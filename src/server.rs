//! The tower-lsp language server.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{analyze, catalog, diag, logic};

/// LSP backend: document store, doc catalog, and the `-C` runner.
pub struct Backend {
    client: Client,
    docs: DashMap<Url, String>,
    catalog: std::sync::RwLock<Vec<catalog::ModuleDoc>>,
    core: std::sync::RwLock<catalog::CoreDocs>,
    src: std::sync::RwLock<Option<String>>,
    opensips_bin: std::sync::RwLock<Option<String>>,
    /// Serializes `opensips -C` runs: one at a time, no process storm.
    check_gate: tokio::sync::Mutex<()>,
    check_timeout: std::sync::RwLock<std::time::Duration>,
}

impl Backend {
    /// Build a backend for one client connection.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            catalog: std::sync::RwLock::new(Vec::new()),
            core: std::sync::RwLock::new(catalog::CoreDocs::default()),
            src: std::sync::RwLock::new(None),
            opensips_bin: std::sync::RwLock::new(None),
            check_gate: tokio::sync::Mutex::new(()),
            check_timeout: std::sync::RwLock::new(logic::resolve_timeout(
                None,
                std::env::var("OPENSIPS_LSP_CHECK_TIMEOUT_MS").ok(),
            )),
        }
    }

    fn doc_line(text: &str, line: u32) -> String {
        text.lines().nth(line as usize).unwrap_or("").to_string()
    }

    fn line_prefix(text: &str, pos: Position) -> String {
        text.lines()
            .nth(pos.line as usize)
            .map(|l| {
                let e = analyze::utf16_to_byte(l, pos.character);
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
        let check_timeout = *self.check_timeout.read().unwrap();
        let out = match tokio::time::timeout(check_timeout, fut).await {
            Ok(r) => r,
            Err(_) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!(
                            "opensips-lsp: '{bin} -C' timed out after {:?} on {path_str}",
                            check_timeout
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
        // opensips reports byte columns; the client expects UTF-16 units
        let doc_text = self.docs.get(uri).map(|t| t.clone()).unwrap_or_default();
        let diags: Vec<Diagnostic> = diag::parse_check_output(&text, rc)
            .into_iter()
            .filter(|d| logic::diag_matches_file(&d.file, &path))
            .map(|d| Diagnostic {
                range: {
                    let lt = Self::doc_line(&doc_text, d.line);
                    Range {
                        start: Position::new(
                            d.line,
                            analyze::byte_to_utf16(&lt, d.col_start as usize),
                        ),
                        end: Position::new(d.line, analyze::byte_to_utf16(&lt, d.col_end as usize)),
                    }
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

        if let Some(ms) = opts.get("checkTimeoutMs").and_then(|v| v.as_u64()) {
            *self.check_timeout.write().unwrap() = logic::resolve_timeout(
                Some(ms),
                std::env::var("OPENSIPS_LSP_CHECK_TIMEOUT_MS").ok(),
            );
        }

        let src = opts
            .get("opensipsSrc")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| std::env::var("OPENSIPS_LSP_SRC").ok())
            .filter(|s| !s.is_empty());
        // the harvest happens in `initialized` so a large tree never
        // delays the initialize handshake
        *self.src.write().unwrap() = src;

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
        let src = self.src.read().unwrap().clone();
        let mut cached = false;
        if let Some(src) = src {
            // the harvest runs off the executor thread and outside the
            // handshake; results are cached per tree fingerprint
            let (harvested, core, hit) = tokio::task::spawn_blocking(move || {
                let p = std::path::Path::new(&src);
                let cache_dir = std::env::var("OPENSIPS_LSP_CACHE_DIR")
                    .map(std::path::PathBuf::from)
                    .ok()
                    .or_else(|| {
                        std::env::var("XDG_CACHE_HOME")
                            .map(std::path::PathBuf::from)
                            .ok()
                            .or_else(|| {
                                std::env::var("HOME")
                                    .map(|h| std::path::PathBuf::from(h).join(".cache"))
                                    .ok()
                            })
                            .map(|c| c.join("opensips-lsp"))
                    });
                if let Some(dir) = &cache_dir
                    && let Some((m, c)) = catalog::load_cached(p, dir)
                {
                    return (m, c, true);
                }
                let out = (catalog::harvest_tree(p), catalog::harvest_core(p));
                if let Some(dir) = &cache_dir {
                    let _ = catalog::save_cache(p, dir, &out.0, &out.1);
                }
                (out.0, out.1, false)
            })
            .await
            .unwrap_or_default();
            *self.catalog.write().unwrap() = harvested;
            *self.core.write().unwrap() = core;
            cached = hit;
        }
        let n = self.catalog.read().unwrap().len();
        let c = self.core.read().unwrap().functions.len();
        let tag = if cached { ", cached" } else { "" };
        self.client
            .log_message(
                MessageType::INFO,
                format!("opensips-lsp ready ({n} documented modules, {c} core functions{tag})"),
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
        let core = self.core.read().unwrap();
        let items: Vec<CompletionItem> = logic::completions_with_core(&cat, &core, &text, &prefix)
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
        let byte_col = analyze::utf16_to_byte(line, pos.character);
        let Some(word) = analyze::word_at(line, byte_col) else {
            return Ok(None);
        };
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        Ok(
            logic::hover_markdown_with_core(&cat, &core, &text, &word).map(|md| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: md,
                }),
                range: None,
            }),
        )
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
        let line_text = Self::doc_line(&text, pos.line);
        let byte_col = analyze::utf16_to_byte(&line_text, pos.character) as u32;
        Ok(logic::definition_of(&text, pos.line, byte_col).map(|d| {
            let def_line = Self::doc_line(&text, d.line);
            let c = analyze::byte_to_utf16(&def_line, d.col as usize);
            GotoDefinitionResponse::Scalar(Location {
                uri,
                range: Range {
                    start: Position::new(d.line, c),
                    end: Position::new(d.line, c),
                },
            })
        }))
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
                    range: {
                        let lt = Self::doc_line(&text, r.line);
                        let c = analyze::byte_to_utf16(&lt, r.col as usize);
                        Range {
                            start: Position::new(r.line, c),
                            end: Position::new(r.line, c),
                        }
                    },
                },
                container_name: None,
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Flat(syms)))
    }
}
