//! The tower-lsp language server.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{analyze, catalog, diag, logic, memo};

/// LSP backend: document store, doc catalog, and the `-C` runner.
pub struct Backend {
    client: Client,
    /// Open documents: (version, full text).
    docs: DashMap<Url, (i32, String)>,
    catalog: std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
    core: std::sync::RwLock<catalog::CoreDocs>,
    src: std::sync::RwLock<Option<String>>,
    opensips_bin: std::sync::RwLock<Option<String>>,
    /// Serializes `opensips -C` runs: one at a time, no process storm.
    check_gate: tokio::sync::Mutex<()>,
    snippet_completions: std::sync::RwLock<bool>,
    max_diagnostics: std::sync::RwLock<usize>,
    cache_dir_opt: std::sync::RwLock<Option<String>>,
    check_timeout: std::sync::RwLock<std::time::Duration>,
    /// Last published `-C` results per document; merged with analyzer
    /// diagnostics on every publish.
    check_diags: std::sync::Arc<DashMap<Url, Vec<Diagnostic>>>,
    /// Fast analyzer diagnostics between saves (init option).
    analyzer_enabled: std::sync::RwLock<bool>,
    /// didChange generation per document: only the latest debounced
    /// analyzer task publishes.
    change_gen: std::sync::Arc<DashMap<Url, u64>>,
    /// Reference-count code lenses on route definitions (init option).
    code_lens_refs: std::sync::RwLock<bool>,
    /// Per-document check generation: a newer check supersedes (and
    /// kills) an older one still queued or running for the same URI.
    check_super: std::sync::Arc<DashMap<Url, u64>>,
    /// Per-(document, version) scanner memoization for hot handlers.
    memo: memo::AnalysisCache,
}

impl Backend {
    /// Build a backend for one client connection.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            docs: DashMap::new(),
            catalog: std::sync::Arc::new(std::sync::RwLock::new(Vec::new())),
            core: std::sync::RwLock::new(catalog::CoreDocs::default()),
            src: std::sync::RwLock::new(None),
            opensips_bin: std::sync::RwLock::new(None),
            check_gate: tokio::sync::Mutex::new(()),
            snippet_completions: std::sync::RwLock::new(true),
            max_diagnostics: std::sync::RwLock::new(100),
            cache_dir_opt: std::sync::RwLock::new(None),
            check_timeout: std::sync::RwLock::new(logic::resolve_timeout(
                None,
                std::env::var("OPENSIPS_LSP_CHECK_TIMEOUT_MS").ok(),
            )),
            check_diags: std::sync::Arc::new(DashMap::new()),
            analyzer_enabled: std::sync::RwLock::new(true),
            change_gen: std::sync::Arc::new(DashMap::new()),
            code_lens_refs: std::sync::RwLock::new(true),
            check_super: std::sync::Arc::new(DashMap::new()),
            memo: memo::AnalysisCache::default(),
        }
    }

    /// Snapshot of open file-scheme buffers, for include resolution
    /// that prefers editor contents over disk.
    fn open_docs_snapshot(&self) -> std::collections::HashMap<std::path::PathBuf, String> {
        self.docs
            .iter()
            .filter_map(|e| {
                let url = e.key();
                if url.scheme() != "file" {
                    return None;
                }
                url.to_file_path().ok().map(|p| (p, e.value().1.clone()))
            })
            .collect()
    }

    /// A file loader over an open-buffer snapshot with a disk
    /// fallback, size-capped so a hostile include stays cheap.
    fn make_loader(
        open: std::collections::HashMap<std::path::PathBuf, String>,
    ) -> impl Fn(&std::path::Path) -> Option<String> {
        move |p: &std::path::Path| {
            if let Some(t) = open.get(p) {
                return Some(t.clone());
            }
            let md = std::fs::metadata(p).ok()?;
            if !md.is_file() || md.len() > 1_048_576 {
                return None;
            }
            std::fs::read_to_string(p).ok()
        }
    }

    /// Analyzer diagnostics for `text`, mapped to LSP (UTF-16) ranges.
    fn analyzer_lsp_diags(
        path: &std::path::Path,
        text: &str,
        loader: &dyn Fn(&std::path::Path) -> Option<String>,
        cat: &[catalog::ModuleDoc],
    ) -> Vec<Diagnostic> {
        let mut all = logic::analyzer_diagnostics(path, text, loader);
        all.extend(logic::catalog_diagnostics(cat, text));
        all.into_iter()
            .map(|d| {
                let lt = Self::doc_line(text, d.line);
                Diagnostic {
                    range: Range {
                        start: Position::new(
                            d.line,
                            analyze::byte_to_utf16(&lt, d.col_start as usize),
                        ),
                        end: Position::new(d.line, analyze::byte_to_utf16(&lt, d.col_end as usize)),
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("opensips-lsp".into()),
                    message: d.message,
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Merge stored `-C` results with fresh analyzer diagnostics and
    /// publish, capped.  Static so the debounce task can call it.
    #[allow(clippy::too_many_arguments)]
    async fn merge_and_publish(
        client: &Client,
        check_map: &DashMap<Url, Vec<Diagnostic>>,
        analyzer_enabled: bool,
        cap: usize,
        uri: &Url,
        version: i32,
        text: &str,
        open: std::collections::HashMap<std::path::PathBuf, String>,
        cat: &std::sync::Arc<std::sync::RwLock<Vec<catalog::ModuleDoc>>>,
        skip_if_empty: bool,
    ) {
        let mut merged = check_map.get(uri).map(|v| v.clone()).unwrap_or_default();
        if analyzer_enabled && let Ok(path) = uri.to_file_path() {
            let loader = Self::make_loader(open);
            let cat = cat.read().unwrap().clone();
            merged.extend(Self::analyzer_lsp_diags(&path, text, &loader, &cat));
        }
        if merged.is_empty() && skip_if_empty {
            return;
        }
        merged.truncate(cap.max(1));
        client
            .publish_diagnostics(uri.clone(), merged, Some(version))
            .await;
    }

    /// Apply the RUNTIME-tunable options (shared by initialize and
    /// workspace/didChangeConfiguration).  Binary/tree/cache paths are
    /// deliberately excluded: those need a restart.
    fn apply_runtime_opts(&self, opts: &serde_json::Value) {
        if let Some(b) = opts.get("snippetCompletions").and_then(|v| v.as_bool()) {
            *self.snippet_completions.write().unwrap() = b;
        }
        if let Some(b) = opts.get("analyzerDiagnostics").and_then(|v| v.as_bool()) {
            *self.analyzer_enabled.write().unwrap() = b;
        }
        if let Some(b) = opts.get("codeLensReferences").and_then(|v| v.as_bool()) {
            *self.code_lens_refs.write().unwrap() = b;
        }
        if let Some(n) = opts.get("maxDiagnostics").and_then(|v| v.as_u64()) {
            *self.max_diagnostics.write().unwrap() = (n as usize).max(1);
        }
        if let Some(ms) = opts.get("checkTimeoutMs").and_then(|v| v.as_u64()) {
            *self.check_timeout.write().unwrap() = logic::resolve_timeout(
                Some(ms),
                std::env::var("OPENSIPS_LSP_CHECK_TIMEOUT_MS").ok(),
            );
        }
    }

    fn doc_line(text: &str, line: u32) -> String {
        text.lines().nth(line as usize).unwrap_or("").to_string()
    }

    /// The include closure rooted at an open document: the document
    /// itself plus transitively included files (open buffers first,
    /// disk fallback).  Non-file documents get a single-entry closure.
    fn closure_for(&self, uri: &Url, text: &str) -> Vec<(std::path::PathBuf, String)> {
        let Ok(path) = uri.to_file_path() else {
            return vec![(std::path::PathBuf::new(), text.to_string())];
        };
        let loader = Self::make_loader(self.open_docs_snapshot());
        logic::include_closure(&path, text, &loader)
    }

    /// The route name under an LSP (UTF-16) position with its
    /// namespace, if any.
    fn route_name_at(text: &str, pos: Position) -> Option<(String, logic::RouteNs)> {
        let line = text.lines().nth(pos.line as usize)?;
        let byte_col = analyze::utf16_to_byte(line, pos.character) as u32;
        logic::route_symbol_ns_at(text, pos.line, byte_col)
    }

    /// UTF-16 range of one route-name occurrence.
    fn occurrence_range(text: &str, l: &analyze::Located) -> Range {
        let lt = Self::doc_line(text, l.line);
        let s = analyze::byte_to_utf16(&lt, l.col as usize);
        let e = analyze::byte_to_utf16(&lt, l.col as usize + l.name.len());
        Range {
            start: Position::new(l.line, s),
            end: Position::new(l.line, e),
        }
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
        if uri.scheme() != "file" {
            return;
        }
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let path_str = path.display().to_string();
        // snapshot the buffer BEFORE the subprocess runs: ranges are
        // mapped through exactly this text, and the publish carries
        // exactly this version
        let (snap_version, snap_text) = self
            .docs
            .get(uri)
            .map(|d| d.clone())
            .unwrap_or((0, String::new()));
        let analyzer_enabled = *self.analyzer_enabled.read().unwrap();
        let cap = *self.max_diagnostics.read().unwrap();
        let Some(bin) = self.opensips_bin.read().unwrap().clone() else {
            // -C disabled: analyzer-only pass.  skip_if_empty keeps the
            // no-checks contract quiet for clean documents.
            self.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                self.open_docs_snapshot(),
                &self.catalog,
                true,
            )
            .await;
            return;
        };
        // latest-wins: this check supersedes any older one for the URI
        let my_gen = {
            let mut e = self.check_super.entry(uri.clone()).or_insert(0);
            *e += 1;
            *e
        };
        let superseded = |m: &DashMap<Url, u64>, u: &Url| m.get(u).map(|g| *g) != Some(my_gen);
        // one -C at a time; a burst of didOpen events must not fork a
        // process per file
        let _gate = self.check_gate.lock().await;
        // a newer check arrived while this one queued: evaporate
        if superseded(&self.check_super, uri) {
            return;
        }
        // test-only hook: slow the check down to make races observable
        if let Some(ms) = std::env::var("OPENSIPS_LSP_TEST_CHECK_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
        {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        }
        // stream stdout+stderr with a byte cap: the timeout bounds
        // seconds, this bounds memory — a flooding checker is killed
        let out_cap = std::env::var("OPENSIPS_LSP_OUTPUT_CAP_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(1_048_576);
        let fut = async {
            let mut child = tokio::process::Command::new(&bin)
                .arg("-C")
                .arg("-f")
                .arg(&path_str)
                // relative includes/module paths resolve against the
                // checker's cwd — run from the cfg's dir like the CLI
                .current_dir(
                    std::path::Path::new(&path_str)
                        .parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or_else(|| std::path::Path::new(".")),
                )
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true)
                .spawn()?;
            use tokio::io::AsyncReadExt;
            async fn read_capped(
                mut s: impl tokio::io::AsyncRead + Unpin,
                cap: usize,
            ) -> std::io::Result<(Vec<u8>, bool)> {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    let n = s.read(&mut chunk).await?;
                    if n == 0 {
                        return Ok((buf, false));
                    }
                    if buf.len() + n > cap {
                        return Ok((buf, true));
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
            let stdout = child.stdout.take().expect("piped");
            let stderr = child.stderr.take().expect("piped");
            let (o, e) = tokio::join!(read_capped(stdout, out_cap), read_capped(stderr, out_cap));
            let (o_buf, o_capped) = o?;
            let (e_buf, e_capped) = e?;
            if o_capped || e_capped {
                let _ = child.kill().await;
                return Ok::<_, std::io::Error>((None, Vec::new(), Vec::new()));
            }
            let status = child.wait().await?;
            Ok((Some(status), o_buf, e_buf))
        };
        let check_timeout = *self.check_timeout.read().unwrap();
        // watch for a newer check while the subprocess runs: dropping
        // `fut` kills the child (kill_on_drop)
        let super_watch = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if superseded(&self.check_super, uri) {
                    break;
                }
            }
        };
        let timed = tokio::select! {
            r = tokio::time::timeout(check_timeout, fut) => r,
            _ = super_watch => return,
        };
        let out = match timed {
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
                // an incomplete check must not leave stale results pinned
                self.check_diags.insert(uri.clone(), Vec::new());
                Self::merge_and_publish(
                    &self.client,
                    &self.check_diags,
                    analyzer_enabled,
                    cap,
                    uri,
                    snap_version,
                    &snap_text,
                    self.open_docs_snapshot(),
                    &self.catalog,
                    false,
                )
                .await;
                return;
            }
        };
        let Ok((status, o_buf, e_buf)) = out else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("opensips-lsp: cannot run '{bin} -C' (configure opensipsPath)"),
                )
                .await;
            self.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                self.open_docs_snapshot(),
                &self.catalog,
                false,
            )
            .await;
            return;
        };
        let Some(status) = status else {
            // capped: the run's output is untrustworthy — clear and log
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!(
                        "opensips-lsp: '{bin} -C' exceeded the output cap ({out_cap} bytes) on {path_str}; run discarded"
                    ),
                )
                .await;
            self.check_diags.insert(uri.clone(), Vec::new());
            Self::merge_and_publish(
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                uri,
                snap_version,
                &snap_text,
                self.open_docs_snapshot(),
                &self.catalog,
                false,
            )
            .await;
            return;
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&o_buf),
            String::from_utf8_lossy(&e_buf)
        );
        let rc = status.code().unwrap_or(-1);
        // the buffer moved while the check ran: results belong to a
        // text that no longer exists — suppress; the next save re-checks
        let current = self.docs.get(uri).map(|d| d.0);
        if current.is_some() && current != Some(snap_version) {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("opensips-lsp: discarding stale check of {path_str} (buffer changed)"),
                )
                .await;
            return;
        }
        // opensips reports byte columns; the client expects UTF-16 units
        let doc_text = snap_text.clone();
        let diags: Vec<Diagnostic> = diag::parse_check_output(&text, rc)
            .into_iter()
            .map(|d| {
                // errors inside include_file targets are re-attributed
                // to the root (at the include directive) instead of
                // being dropped: a broken config must never render as
                // clean
                let (line, c0, c1, message) = if logic::diag_matches_file(&d.file, &path) {
                    (d.line, d.col_start, d.col_end, d.message)
                } else {
                    let f = logic::attribute_foreign_diag(
                        &path, &doc_text, &d.file, d.line, &d.message,
                    );
                    (f.line, f.col_start, f.col_end, f.message)
                };
                Diagnostic {
                    range: {
                        let lt = Self::doc_line(&doc_text, line);
                        Range {
                            start: Position::new(line, analyze::byte_to_utf16(&lt, c0 as usize)),
                            end: Position::new(line, analyze::byte_to_utf16(&lt, c1 as usize)),
                        }
                    },
                    severity: Some(match d.severity {
                        diag::Severity::Error => DiagnosticSeverity::ERROR,
                        diag::Severity::Warning => DiagnosticSeverity::WARNING,
                    }),
                    source: Some("opensips -C".into()),
                    message,
                    ..Default::default()
                }
            })
            .collect();
        let mut diags = diags;
        if diags.len() > cap {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "opensips-lsp: {} diagnostics on {path_str}, publishing the first {cap} (maxDiagnostics)",
                        diags.len()
                    ),
                )
                .await;
            diags.truncate(cap);
        }
        self.check_diags.insert(uri.clone(), diags);
        Self::merge_and_publish(
            &self.client,
            &self.check_diags,
            analyzer_enabled,
            cap,
            uri,
            snap_version,
            &snap_text,
            self.open_docs_snapshot(),
            &self.catalog,
            false,
        )
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

        self.apply_runtime_opts(&opts);
        if let Some(d) = opts.get("cacheDir").and_then(|v| v.as_str())
            && !d.is_empty()
        {
            *self.cache_dir_opt.write().unwrap() = Some(d.to_string());
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
                    trigger_characters: Some(vec!["\"".into(), "$".into()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into()]),
                    retrigger_characters: None,
                    work_done_progress_options: Default::default(),
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                document_highlight_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::FUNCTION,
                                    SemanticTokenType::VARIABLE,
                                ],
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: Some(true),
                            work_done_progress_options: Default::default(),
                        },
                    ),
                ),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        let src = self.src.read().unwrap().clone();
        let mut cached = false;
        if let Some(src) = src {
            // editors showing workDoneProgress see the harvest as a
            // background job; the create round-trip is time-bounded so
            // clients that never answer cannot stall startup
            let token = NumberOrString::String("opensips-lsp/harvest".into());
            let progress = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                self.client.send_request::<request::WorkDoneProgressCreate>(
                    WorkDoneProgressCreateParams {
                        token: token.clone(),
                    },
                ),
            )
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false);
            if progress {
                self.client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token: token.clone(),
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::Begin(
                            WorkDoneProgressBegin {
                                title: "Harvesting OpenSIPS documentation".into(),
                                cancellable: Some(false),
                                message: Some(src.clone()),
                                percentage: None,
                            },
                        )),
                    })
                    .await;
            }
            // the harvest runs off the executor thread and outside the
            // handshake; results are cached per tree fingerprint
            let cache_opt = self.cache_dir_opt.read().unwrap().clone();
            let src_tree = src.clone();
            let (harvested, core, hit) = tokio::task::spawn_blocking(move || {
                let p = std::path::Path::new(&src_tree);
                let cache_dir = cache_opt
                    .map(std::path::PathBuf::from)
                    .or_else(|| {
                        std::env::var("OPENSIPS_LSP_CACHE_DIR")
                            .map(std::path::PathBuf::from)
                            .ok()
                    })
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
            let empty = harvested.is_empty();
            *self.catalog.write().unwrap() = harvested;
            *self.core.write().unwrap() = core;
            cached = hit;
            if progress {
                self.client
                    .send_notification::<notification::Progress>(ProgressParams {
                        token,
                        value: ProgressParamsValue::WorkDone(WorkDoneProgress::End(
                            WorkDoneProgressEnd { message: None },
                        )),
                    })
                    .await;
            }
            // a tree the user explicitly configured that documents
            // nothing is a misconfiguration, not a quiet default
            if empty {
                self.client
                    .show_message(
                        MessageType::WARNING,
                        format!(
                            "opensips-lsp: '{src}' yields no module documentation —                              check that opensipsSrc points at an OpenSIPS source tree"
                        ),
                    )
                    .await;
            }
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

    async fn did_change_configuration(&self, p: DidChangeConfigurationParams) {
        // accept both the flat init-option shape and the VS Code
        // nested one ({"opensipsLsp": {...}})
        let s = &p.settings;
        let opts = s.get("opensipsLsp").unwrap_or(s);
        self.apply_runtime_opts(opts);
        // re-publish every open document under the new configuration
        let analyzer_enabled = *self.analyzer_enabled.read().unwrap();
        let cap = *self.max_diagnostics.read().unwrap();
        let open: Vec<(Url, i32, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().0, e.value().1.clone()))
            .collect();
        for (uri, version, text) in open {
            Self::merge_and_publish(
                &self.client,
                &self.check_diags,
                analyzer_enabled,
                cap,
                &uri,
                version,
                &text,
                self.open_docs_snapshot(),
                &self.catalog,
                false,
            )
            .await;
        }
    }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri = p.text_document.uri;
        self.docs
            .insert(uri.clone(), (p.text_document.version, p.text_document.text));
        self.check(&uri).await;
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let Some(change) = p.content_changes.into_iter().last() else {
            return;
        };
        let uri = p.text_document.uri;
        let version = p.text_document.version;
        self.docs
            .insert(uri.clone(), (version, change.text.clone()));
        // debounced analyzer pass: fast feedback between saves
        if !*self.analyzer_enabled.read().unwrap() || uri.scheme() != "file" {
            return;
        }
        let generation = {
            let mut e = self.change_gen.entry(uri.clone()).or_insert(0);
            *e += 1;
            *e
        };
        let gen_map = self.change_gen.clone();
        let check_map = self.check_diags.clone();
        let cat_arc = self.catalog.clone();
        let client = self.client.clone();
        let open = self.open_docs_snapshot();
        let cap = *self.max_diagnostics.read().unwrap();
        let debounce = std::env::var("OPENSIPS_LSP_ANALYZER_DEBOUNCE_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(300);
        let text = change.text;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(debounce)).await;
            // superseded by a newer edit: let the newest task publish
            if gen_map.get(&uri).map(|g| *g) != Some(generation) {
                return;
            }
            Backend::merge_and_publish(
                &client, &check_map, true, cap, &uri, version, &text, open, &cat_arc, false,
            )
            .await;
        });
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        self.check(&p.text_document.uri).await;
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        self.docs.remove(&p.text_document.uri);
        self.check_diags.remove(&p.text_document.uri);
        self.change_gen.remove(&p.text_document.uri);
        self.memo.evict(p.text_document.uri.as_str());
    }

    async fn completion(&self, p: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let pos = p.text_document_position.position;
        let prefix = Self::line_prefix(&text, pos);
        // typed `$` + tail: completions REPLACE the typed token (labels
        // carry the `$`, so plain insertion would double it)
        let pvar_edit_range = logic::pvar_tail(&prefix).map(|n| {
            // the tail is ASCII, so bytes == UTF-16 units
            Range {
                start: Position::new(pos.line, pos.character.saturating_sub(n as u32)),
                end: pos,
            }
        });
        let files = self.closure_for(&uri, &text);
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        let snippets = *self.snippet_completions.read().unwrap();
        let items: Vec<CompletionItem> =
            logic::completions_with_core_files(&cat, &core, &files, &prefix)
                .into_iter()
                .map(|c| {
                    // functions insert tabstop snippets unless disabled
                    let (insert_text, insert_text_format) =
                        if snippets && c.kind == logic::CompKind::Function {
                            if c.detail.contains("()") {
                                (
                                    Some(format!("{}()$0", c.label)),
                                    Some(InsertTextFormat::SNIPPET),
                                )
                            } else {
                                (
                                    Some(format!("{}($1)$0", c.label)),
                                    Some(InsertTextFormat::SNIPPET),
                                )
                            }
                        } else {
                            (None, None)
                        };
                    CompletionItem {
                        text_edit: pvar_edit_range.map(|range| {
                            CompletionTextEdit::Edit(TextEdit {
                                range,
                                new_text: c.label.clone(),
                            })
                        }),
                        insert_text,
                        insert_text_format,
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
                    }
                })
                .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, p: HoverParams) -> Result<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
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
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let line_text = Self::doc_line(&text, pos.line);
        let byte_col = analyze::utf16_to_byte(&line_text, pos.character) as u32;
        if let Some(d) = logic::definition_of(&text, pos.line, byte_col) {
            let def_line = Self::doc_line(&text, d.line);
            let c = analyze::byte_to_utf16(&def_line, d.col as usize);
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri,
                range: Range {
                    start: Position::new(d.line, c),
                    end: Position::new(d.line, c),
                },
            })));
        }
        // not defined in this file: search the include closure
        let Some(name) = logic::route_symbol_at(&text, pos.line, byte_col) else {
            return Ok(None);
        };
        let files = self.closure_for(&uri, &text);
        for (path, ftext) in files.iter().skip(1) {
            if let Some(d) = analyze::route_blocks(ftext)
                .into_iter()
                .filter(|b| b.kind == "route")
                .map(|b| analyze::Located {
                    name: b.name,
                    line: b.line,
                    col: b.col,
                })
                .find(|d| d.name == name)
                && let Ok(target) = Url::from_file_path(path)
            {
                let def_line = Self::doc_line(ftext, d.line);
                let c = analyze::byte_to_utf16(&def_line, d.col as usize);
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target,
                    range: Range {
                        start: Position::new(d.line, c),
                        end: Position::new(d.line, c),
                    },
                })));
            }
        }
        Ok(None)
    }

    async fn document_symbol(
        &self,
        p: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some((version, text)) = self.docs.get(&p.text_document.uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        let analysis = self
            .memo
            .get_or_compute(p.text_document.uri.as_str(), version, &text);
        #[allow(deprecated)]
        let syms: Vec<DocumentSymbol> = analysis
            .blocks
            .iter()
            .map(|b| {
                let lt = Self::doc_line(&text, b.line);
                let start = Position::new(b.line, analyze::byte_to_utf16(&lt, b.col as usize));
                let et = Self::doc_line(&text, b.end_line);
                let end =
                    Position::new(b.end_line, analyze::byte_to_utf16(&et, b.end_col as usize));
                DocumentSymbol {
                    name: if b.name.is_empty() {
                        if b.kind == "route" {
                            "route (main)".into()
                        } else {
                            b.kind.clone()
                        }
                    } else {
                        format!("{}[{}]", b.kind, b.name)
                    },
                    detail: None,
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: Range { start, end },
                    selection_range: Range { start, end: start },
                    children: None,
                }
            })
            .collect();
        Ok(Some(DocumentSymbolResponse::Nested(syms)))
    }

    async fn signature_help(&self, p: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let prefix = Self::line_prefix(&text, pos);
        let cat = self.catalog.read().unwrap();
        let core = self.core.read().unwrap();
        Ok(
            logic::signature_at(&cat, &core, &text, &prefix).map(|(sig, doc, active)| {
                // parameter labels: TOP-LEVEL comma-separated pieces
                // (nested parens like `$json(a)` stay intact)
                let params: Vec<ParameterInformation> = logic::split_params(&sig)
                    .into_iter()
                    .map(|s| ParameterInformation {
                        label: ParameterLabel::Simple(s),
                        documentation: None,
                    })
                    .collect();
                SignatureHelp {
                    signatures: vec![SignatureInformation {
                        label: sig,
                        documentation: (!doc.is_empty()).then_some(Documentation::MarkupContent(
                            MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: doc,
                            },
                        )),
                        parameters: (!params.is_empty()).then_some(params),
                        active_parameter: Some(active),
                    }],
                    active_signature: Some(0),
                    active_parameter: Some(active),
                }
            }),
        )
    }

    async fn references(&self, p: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_name_at(&text, pos) else {
            return Ok(None);
        };
        let include_decl = p.context.include_declaration;
        // occurrences across the whole include closure, not just the
        // current buffer
        let files = self.closure_for(&uri, &text);
        let mut locs = Vec::new();
        for (path, ftext) in &files {
            let Ok(furi) = Url::from_file_path(path) else {
                continue;
            };
            for (l, is_def) in logic::ns_occurrences(ftext, &name, &ns) {
                if include_decl || !is_def {
                    locs.push(Location {
                        uri: furi.clone(),
                        range: Self::occurrence_range(ftext, &l),
                    });
                }
            }
        }
        Ok(Some(locs))
    }

    async fn document_highlight(
        &self,
        p: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_name_at(&text, pos) else {
            return Ok(None);
        };
        let hls = logic::ns_occurrences(&text, &name, &ns)
            .into_iter()
            .map(|(l, is_def)| DocumentHighlight {
                range: Self::occurrence_range(&text, &l),
                kind: Some(if is_def {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                }),
            })
            .collect();
        Ok(Some(hls))
    }

    async fn rename(&self, p: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let new_name = p.new_name;
        if !logic::valid_route_name(&new_name) {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "'{new_name}' is not a legal route name ([A-Za-z0-9_.:-]+)"
            )));
        }
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Some((name, ns)) = Self::route_name_at(&text, pos) else {
            return Ok(None);
        };
        // rewrite every occurrence of the symbol's namespace in the
        // whole include closure
        let files = self.closure_for(&uri, &text);
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();
        for (path, ftext) in &files {
            let Ok(furi) = Url::from_file_path(path) else {
                continue;
            };
            let edits: Vec<TextEdit> = logic::ns_occurrences(ftext, &name, &ns)
                .into_iter()
                .map(|(l, _)| TextEdit {
                    range: Self::occurrence_range(ftext, &l),
                    new_text: new_name.clone(),
                })
                .collect();
            if !edits.is_empty() {
                changes.insert(furi, edits);
            }
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }))
    }

    async fn symbol(&self, p: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
        let query = p.query.to_lowercase();
        let mut seen: std::collections::HashSet<std::path::PathBuf> =
            std::collections::HashSet::new();
        let mut out = Vec::new();
        // every open document plus its include closure, deduplicated
        let open: Vec<(Url, String)> = self
            .docs
            .iter()
            .map(|e| (e.key().clone(), e.value().1.clone()))
            .collect();
        for (uri, text) in open {
            for (path, ftext) in self.closure_for(&uri, &text) {
                if !seen.insert(path.clone()) {
                    continue;
                }
                let Ok(furi) = Url::from_file_path(&path) else {
                    continue;
                };
                for b in analyze::route_blocks(&ftext) {
                    if b.name.is_empty() || !b.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    let lt = Self::doc_line(&ftext, b.line);
                    let c = analyze::byte_to_utf16(&lt, b.col as usize);
                    #[allow(deprecated)]
                    out.push(SymbolInformation {
                        name: format!("{}[{}]", b.kind, b.name),
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri: furi.clone(),
                            range: Range {
                                start: Position::new(b.line, c),
                                end: Position::new(b.line, c),
                            },
                        },
                        container_name: None,
                    });
                    if out.len() >= 256 {
                        return Ok(Some(out));
                    }
                }
            }
        }
        Ok(Some(out))
    }

    async fn prepare_rename(
        &self,
        p: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let pos = p.position;
        let Some((name, ns)) = Self::route_name_at(&text, pos) else {
            return Ok(None);
        };
        // the exact occurrence span under the cursor becomes the edit
        // range the editor pre-selects
        let hit = logic::ns_occurrences(&text, &name, &ns)
            .into_iter()
            .map(|(l, _)| Self::occurrence_range(&text, &l))
            .find(|r| {
                r.start.line == pos.line
                    && r.start.character <= pos.character
                    && pos.character <= r.end.character
            });
        Ok(
            hit.map(|range| PrepareRenameResponse::RangeWithPlaceholder {
                range,
                placeholder: name,
            }),
        )
    }

    async fn document_link(&self, p: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = p.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let Ok(path) = uri.to_file_path() else {
            return Ok(None);
        };
        let base = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let mut out = Vec::new();
        for inc in analyze::includes(&text) {
            let lt = Self::doc_line(&text, inc.line);
            // locate the quoted path text after the directive keyword
            let Some(rel) = lt
                .get(inc.col as usize..)
                .and_then(|s| s.find(inc.name.as_str()))
                .map(|i| inc.col as usize + i)
            else {
                continue;
            };
            let Ok(target) = Url::from_file_path(base.join(&inc.name)) else {
                continue;
            };
            out.push(DocumentLink {
                range: Range {
                    start: Position::new(inc.line, analyze::byte_to_utf16(&lt, rel)),
                    end: Position::new(inc.line, analyze::byte_to_utf16(&lt, rel + inc.name.len())),
                },
                target: Some(target),
                tooltip: None,
                data: None,
            });
        }
        Ok(Some(out))
    }

    async fn semantic_tokens_range(
        &self,
        p: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let r = p.range;
        let data = logic::encode_semantic_tokens_range(
            &text,
            r.start.line,
            r.start.character,
            r.end.line,
            r.end.character,
        )
        .chunks(5)
        .map(|c| SemanticToken {
            delta_line: c[0],
            delta_start: c[1],
            length: c[2],
            token_type: c[3],
            token_modifiers_bitset: c[4],
        })
        .collect();
        Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn semantic_tokens_full(
        &self,
        p: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some(text) = self.docs.get(&p.text_document.uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let data = logic::encode_semantic_tokens(&text)
            .chunks(5)
            .map(|c| SemanticToken {
                delta_line: c[0],
                delta_start: c[1],
                length: c[2],
                token_type: c[3],
                token_modifiers_bitset: c[4],
            })
            .collect();
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn code_action(&self, p: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = p.text_document.uri;
        let Some(text) = self.docs.get(&uri).map(|d| d.1.clone()) else {
            return Ok(None);
        };
        let cat = self.catalog.read().unwrap();
        let mut out: Vec<CodeActionOrCommand> = Vec::new();
        for d in &p.context.diagnostics {
            for f in logic::quick_fixes(&cat, &text, &d.message) {
                let pos = Position::new(f.line, f.col);
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    uri.clone(),
                    vec![TextEdit {
                        range: Range {
                            start: pos,
                            end: pos,
                        },
                        new_text: f.insert,
                    }],
                );
                out.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: f.title,
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![d.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    ..Default::default()
                }));
            }
        }
        Ok(Some(out))
    }

    async fn code_lens(&self, p: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        if !*self.code_lens_refs.read().unwrap() {
            return Ok(None);
        }
        let uri = p.text_document.uri;
        let Some((version, text)) = self.docs.get(&uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        let files = self.closure_for(&uri, &text);
        let analysis = self.memo.get_or_compute(uri.as_str(), version, &text);
        let lenses = analysis
            .blocks
            .iter()
            // only request routes are route()-callable; other kinds'
            // arming isn't tracked, so a count would mislead
            .filter(|b| b.kind == "route" && !b.name.is_empty())
            .map(|b| {
                let n: usize = files
                    .iter()
                    .map(|(_, t)| {
                        analyze::route_refs(t)
                            .into_iter()
                            .filter(|r| r.name == b.name)
                            .count()
                    })
                    .sum();
                let lt = Self::doc_line(&text, b.name_line);
                let c = analyze::byte_to_utf16(&lt, b.name_col as usize);
                CodeLens {
                    range: Range {
                        start: Position::new(b.name_line, c),
                        end: Position::new(b.name_line, c),
                    },
                    command: Some(Command {
                        title: if n == 1 {
                            "1 reference".into()
                        } else {
                            format!("{n} references")
                        },
                        command: String::new(),
                        arguments: None,
                    }),
                    data: None,
                }
            })
            .collect();
        Ok(Some(lenses))
    }

    async fn folding_range(&self, p: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some((version, text)) = self.docs.get(&p.text_document.uri).map(|d| d.clone()) else {
            return Ok(None);
        };
        let analysis = self
            .memo
            .get_or_compute(p.text_document.uri.as_str(), version, &text);
        let ranges: Vec<FoldingRange> = analysis
            .blocks
            .iter()
            .filter(|b| b.end_line > b.line)
            .map(|b| FoldingRange {
                start_line: b.line,
                start_character: None,
                end_line: b.end_line,
                end_character: None,
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: None,
            })
            .collect();
        Ok(Some(ranges))
    }
}
