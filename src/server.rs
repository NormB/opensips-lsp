//! The tower-lsp language server.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{analyze, catalog, diag, logic};

pub struct Backend {
    client: Client,
    docs: DashMap<Url, String>,
    catalog: std::sync::RwLock<Vec<catalog::ModuleDoc>>,
    opensips_bin: std::sync::RwLock<Option<String>>,
    /// Serializes `opensips -C` runs: one at a time, no process storm.
    check_gate: tokio::sync::Mutex<()>,
    check_timeout: std::time::Duration,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let check_timeout = std::env::var("OPENSIPS_LSP_CHECK_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(std::time::Duration::from_millis)
            .unwrap_or(std::time::Duration::from_secs(10));
        Self {
            client,
            docs: DashMap::new(),
            catalog: std::sync::RwLock::new(Vec::new()),
            opensips_bin: std::sync::RwLock::new(None),
            check_gate: tokio::sync::Mutex::new(()),
            check_timeout,
        }
    }

    fn line_prefix(text: &str, pos: Position) -> String {
        text.lines()
            .nth(pos.line as usize)
            .map(|l| {
                let end = (pos.character as usize).min(l.len());
                // avoid slicing inside a UTF-8 char
                let mut e = end;
                while e > 0 && !l.is_char_boundary(e) {
                    e -= 1;
                }
                l[..e].to_string()
            })
            .unwrap_or_default()
    }

    async fn check(&self, uri: &Url) {
        let Some(bin) = self.opensips_bin.read().unwrap().clone() else {
            return;
        };
        if uri.scheme() != "file" {
            return;
        }
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let path_str = path.display().to_string();
        // one -C at a time; a burst of didOpen events must not fork a
        // process per file
        let _gate = self.check_gate.lock().await;
        let fut = tokio::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(&path_str)
            .kill_on_drop(true)
            .output();
        let out = match tokio::time::timeout(self.check_timeout, fut).await {
            Ok(r) => r,
            Err(_) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "opensips-lsp: '{bin} -C' timed out after {:?} on {path_str}",
                            self.check_timeout
                        ),
                    )
                    .await;
                return;
            }
        };
        let Ok(out) = out else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("opensips-lsp: cannot run '{bin} -C' (configure opensipsPath)"),
                )
                .await;
            return;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let rc = out.status.code().unwrap_or(-1);
        let diags: Vec<Diagnostic> = diag::parse_check_output(&text, rc)
            .into_iter()
            .filter(|d| logic::diag_matches_file(&d.file, &path))
            .map(|d| Diagnostic {
                range: Range {
                    start: Position::new(d.line, d.col_start),
                    end: Position::new(d.line, d.col_end),
                },
                severity: Some(match d.severity {
                    diag::Severity::Error => DiagnosticSeverity::ERROR,
                    diag::Severity::Warning => DiagnosticSeverity::WARNING,
                }),
                source: Some("opensips -C".into()),
                message: d.message,
                ..Default::default()
            })
            .collect();
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, p: InitializeParams) -> Result<InitializeResult> {
        // Resolution order: initializationOptions, then environment.
        let opts = p.initialization_options.unwrap_or_default();
        let bin = logic::resolve_bin(
            opts.get("opensipsPath").and_then(|v| v.as_str()),
            std::env::var("OPENSIPS_LSP_BIN").ok(),
        );
        *self.opensips_bin.write().unwrap() = bin;

        let src = opts
            .get("opensipsSrc")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("OPENSIPS_LSP_SRC").ok())
            .filter(|s| !s.is_empty());
        if let Some(src) = src {
            // a large tree takes seconds to harvest; keep it off the
            // async executor thread
            let harvested = tokio::task::spawn_blocking(move || {
                catalog::harvest_tree(std::path::Path::new(&src))
            })
            .await
            .unwrap_or_default();
            *self.catalog.write().unwrap() = harvested;
        }

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "opensips-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["\"".into()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let n = self.catalog.read().unwrap().len();
        self.client
            .log_message(
                MessageType::INFO,
                format!("opensips-lsp ready ({n} documented modules)"),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri = p.text_document.uri;
        self.docs.insert(uri.clone(), p.text_document.text);
        self.check(&uri).await;
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        if let Some(change) = p.content_changes.into_iter().last() {
            self.docs.insert(p.text_document.uri, change.text);
        }
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        self.check(&p.text_document.uri).await;
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        self.docs.remove(&p.text_document.uri);
    }

    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        let prefix = Self::line_prefix(&text, p.text_document_position.position);
        let cat = self.catalog.read().unwrap();
        let items: Vec<CompletionItem> = logic::completions(&cat, &text, &prefix)
            .into_iter()
            .map(|c| CompletionItem {
                label: c.label,
                detail: Some(c.detail),
                documentation: (!c.doc.is_empty()).then_some({
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: c.doc,
                    })
                }),
                kind: Some(match c.kind {
                    logic::CompKind::Module => CompletionItemKind::MODULE,
                    logic::CompKind::Param => CompletionItemKind::PROPERTY,
                    logic::CompKind::Function => CompletionItemKind::FUNCTION,
                    logic::CompKind::Route => CompletionItemKind::REFERENCE,
                    logic::CompKind::Keyword => CompletionItemKind::KEYWORD,
                }),
                ..Default::default()
            })
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, p: HoverParams) -> Result<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        let Some(line) = text.lines().nth(pos.line as usize) else {
            return Ok(None);
        };
        let Some(word) = analyze::word_at(line, pos.character as usize) else {
            return Ok(None);
        };
        let cat = self.catalog.read().unwrap();
        Ok(logic::hover_markdown(&cat, &text, &word).map(|md| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        p: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        Ok(
            logic::definition_of(&text, pos.line, pos.character).map(|d| {
                GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: Range {
                        start: Position::new(d.line, d.col),
                        end: Position::new(d.line, d.col),
                    },
                })
            }),
        )
    }

    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        #[allow(deprecated)]
        let syms: Vec<SymbolInformation> = analyze::route_defs(&text)
            .into_iter()
            .map(|r| SymbolInformation {
                name: if r.name.is_empty() {
                    "route (main)".into()
                } else {
                    format!("route[{}]", r.name)
                },
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                location: Location {
                    uri: p.text_document.uri.clone(),
                    range: Range {
                        start: Position::new(r.line, r.col),
                        end: Position::new(r.line, r.col),
                    },
                },
                container_name: None,
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Flat(syms)))
    }
}
