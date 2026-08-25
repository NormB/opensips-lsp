//! `opensips-lsp check` — the analyzer (plus the real `opensips -C`
//! when a binary is configured) as a CLI for CI pipelines and git
//! hooks.  Exit codes: 0 clean (warnings allowed), 1 findings
//! (errors, or warnings under `--strict`), 2 usage/read failures.

use crate::{catalog, diag, logic};

fn disk_loader(p: &std::path::Path) -> Option<String> {
    logic::read_config(p)
}

/// The include graph the given files sit in.
///
/// `check` is what a git hook and a CI job run, and it has to reach
/// the same conclusion the editor does about which file is part of
/// which — otherwise a configuration is green in one and red in the
/// other.  The server scans the workspace folders; here the workspace
/// is the directory holding the files that were named, so passing a
/// fragment alone still finds the root sitting beside it.
fn graph_for(files: &[String]) -> logic::IncludeGraph {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for f in files {
        let p = std::path::Path::new(f);
        let dir = p
            .parent()
            .filter(|d| !d.as_os_str().is_empty())
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        if !roots.contains(&dir) {
            roots.push(dir);
        }
    }
    let (mut configs, _) = logic::scan_configs(&roots, 500);
    // and the directories above, without recursing into them: the
    // root that includes `inc/routes.cfg` is almost always the config
    // sitting one level up, and a hook is often handed the fragment
    // alone because that is the file that changed
    for dir in &roots {
        let mut up = dir.parent();
        for _ in 0..3 {
            let Some(parent) = up else { break };
            for c in logic::configs_in_dir(parent) {
                if !configs.contains(&c) {
                    configs.push(c);
                }
            }
            up = parent.parent();
        }
    }
    let scanned: Vec<(std::path::PathBuf, String)> = configs
        .into_iter()
        .filter_map(|p| logic::read_config(&p).map(|t| (p, t)))
        .collect();
    logic::IncludeGraph::build(&scanned)
}

/// Run the `check` subcommand over `args` (everything after `check`).
/// Prints findings as `file:line:col: severity: message` (1-based).
pub fn run_check(args: &[String]) -> i32 {
    let mut strict = false;
    let mut bin_flag: Option<String> = None;
    let mut files: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--strict" => strict = true,
            "--bin" => match it.next() {
                Some(b) => bin_flag = Some(b.clone()),
                None => {
                    eprintln!("--bin needs a path");
                    return 2;
                }
            },
            _ if a.starts_with("--") => {
                eprintln!("unknown flag {a}");
                return 2;
            }
            _ => files.push(a.clone()),
        }
    }
    if files.is_empty() {
        eprintln!("usage: opensips-lsp check [--strict] [--bin <opensips>] <file>...");
        return 2;
    }
    let bin = logic::resolve_bin(bin_flag.as_deref(), std::env::var("OPENSIPS_LSP_BIN").ok());

    let graph = graph_for(&files);

    // The editor checks `modparam` against a catalogue; a CI job and a
    // git hook running `check` were not, so the same configuration was
    // green here and warned about in the editor. `OPENSIPS_LSP_SRC` is
    // the same fallback the server reads, so both resolve one source.
    let (modules, origin) = match std::env::var("OPENSIPS_LSP_SRC")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        Some(src) => {
            let harvested = catalog::harvest_tree(std::path::Path::new(&src));
            if harvested.is_empty() {
                eprintln!(
                    "opensips-lsp: '{src}' yields no module documentation — \
                     falling back to the built-in catalogue"
                );
                (
                    catalog::builtin_modules().modules.clone(),
                    catalog::CatalogOrigin::BuiltIn(catalog::builtin_modules().version.clone()),
                )
            } else {
                (harvested, catalog::CatalogOrigin::ConfiguredTree)
            }
        }
        None => (
            catalog::builtin_modules().modules.clone(),
            catalog::CatalogOrigin::BuiltIn(catalog::builtin_modules().version.clone()),
        ),
    };
    // Findings go to stdout as `file:line:col: sev: msg` and are
    // parsed by other tools; a header among them would break that, so
    // what the run judged against goes to stderr.
    eprintln!(
        "opensips-lsp: modparam checks against {}",
        origin.describe()
    );

    let mut errors = 0usize;
    let mut warnings = 0usize;
    for f in &files {
        let path = std::path::Path::new(f);
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("{f}: cannot read");
            return 2;
        };
        let mut findings: Vec<(u32, u32, bool, String)> = Vec::new();
        // A file another config includes is a fragment, not a
        // program: analysed on its own it reports every route its
        // parent defines as undefined, and under `--strict` that
        // fails the build for a configuration that is correct.
        let root = graph.analysis_root(path);
        let closure = match root.as_ref().and_then(|r| disk_loader(r).map(|t| (r, t))) {
            Some((r, rt)) => {
                let mut files = logic::include_closure(r, &rt, &disk_loader);
                match files.iter().position(|(p, _)| p == path) {
                    Some(i) => {
                        files[i].1 = text.clone();
                        let own = files.remove(i);
                        files.insert(0, own);
                        files
                    }
                    // past the closure's bounds: its own context still
                    // counts, the root's is added to it
                    None => {
                        let mut merged = logic::include_closure(path, &text, &disk_loader);
                        for (p, t) in files {
                            if !merged.iter().any(|(q, _)| *q == p) {
                                merged.push((p, t));
                            }
                        }
                        merged
                    }
                }
            }
            None => logic::include_closure(path, &text, &disk_loader),
        };
        // fast analyzer pass (warnings)
        for d in logic::analyzer_diagnostics_in_closure(&closure, path, &text) {
            findings.push((d.line, d.col_start, false, d.message));
        }
        // the same catalogue check the editor runs
        for d in logic::catalog_diagnostics(&modules, &origin, &text) {
            findings.push((d.line, d.col_start, false, d.message));
        }
        // the real parser, when configured
        if let Some(bin) = &bin {
            let out = std::process::Command::new(bin)
                .arg("-C")
                .arg("-f")
                .arg(path)
                .current_dir(path.parent().unwrap_or(std::path::Path::new(".")))
                .output();
            match out {
                Ok(out) => {
                    let rc = out.status.code().unwrap_or(-1);
                    let all = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    for d in diag::parse_check_output(&all, rc) {
                        let is_err = d.severity == diag::Severity::Error;
                        if logic::diag_matches_file(&d.file, path) {
                            findings.push((d.line, d.col_start, is_err, d.message));
                        } else {
                            let fd = logic::attribute_foreign_diag(
                                path, &text, &d.file, d.line, &d.message,
                            );
                            findings.push((fd.line, fd.col_start, is_err, fd.message));
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{f}: cannot run '{bin} -C': {e}");
                    return 2;
                }
            }
        }
        findings.sort_by_key(|(l, c, _, _)| (*l, *c));
        for (line, col, is_err, msg) in findings {
            let sev = if is_err { "error" } else { "warning" };
            println!("{f}:{}:{}: {sev}: {msg}", line + 1, col + 1);
            if is_err {
                errors += 1;
            } else {
                warnings += 1;
            }
        }
    }
    if errors > 0 || (strict && warnings > 0) {
        1
    } else {
        0
    }
}
