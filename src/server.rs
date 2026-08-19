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
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            catalog: std::sync::RwLock::new(Vec::new()),
            opensips_bin: std::sync::RwLock::new(None),
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
        let out = tokio::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(&path_str)
            .output()
            .await;
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
            .filter(|d| d.file.is_empty() || d.file == path_str)
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
        let bin = opts
            .get("opensipsPath")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("OPENSIPS_LSP_BIN").ok())
            .filter(|s| !s.is_empty())
            .or_else(|| Some("opensips".to_string()));
        *self.opensips_bin.write().unwrap() = bin;

        let src = opts
            .get("opensipsSrc")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("OPENSIPS_LSP_SRC").ok())
            .filter(|s| !s.is_empty());
        if let Some(src) = src {
            *self.catalog.write().unwrap() = catalog::harvest_tree(std::path::Path::new(&src));
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
                documentation: (!c.doc.is_empty()).then(|| {
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
