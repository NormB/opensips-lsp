//! Real-corpus sweep: every documented example and sample cfg that
//! ships with OpenSIPS (the same corpus opensips.org's Script Samples
//! render), from the tree in OPENSIPS_LSP_TEST_TREE.
//!
//! Older documented examples are KNOWN to carry syntax problems, so
//! validity is data, not an assertion. The pinned invariants are:
//!   - the analyzer never panics on any corpus file
//!   - with OPENSIPS_LSP_TEST_BIN set: every file the real parser
//!     rejects yields at least one diagnostic (no silent failures),
//!     and every accepted file yields none

use opensips_lsp::{analyze, diag};

fn corpus(tree: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut push_dir = |d: std::path::PathBuf, recurse: bool| {
        let mut stack = vec![d];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && recurse {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "cfg") {
                    out.push(p);
                }
            }
        }
    };
    push_dir(tree.join("examples"), true);
    push_dir(tree.join("etc"), false);
    if let Ok(rd) = std::fs::read_dir(tree.join("modules")) {
        for e in rd.flatten() {
            push_dir(e.path().join("samples"), false);
        }
    }
    out.sort();
    out
}

#[test]
fn corpus_sweep_no_panics_and_no_silent_failures() {
    let Ok(tree) = std::env::var("OPENSIPS_LSP_TEST_TREE") else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_TREE not set");
        return;
    };
    let tree = std::path::PathBuf::from(tree);
    let files = corpus(&tree);
    assert!(
        files.len() >= 20,
        "expected a real corpus, found {} cfg files",
        files.len()
    );

    let bin = std::env::var("OPENSIPS_LSP_TEST_BIN").ok();
    let (mut ok, mut broken, mut mods_total, mut routes_total) = (0usize, 0usize, 0usize, 0usize);

    for f in &files {
        let text = String::from_utf8_lossy(&std::fs::read(f).unwrap()).into_owned();
        // invariant 1: the analyzer handles every real-world file
        let mods = analyze::loaded_modules(&text);
        let defs = analyze::route_defs(&text);
        let _ = analyze::route_refs(&text);
        mods_total += mods.len();
        routes_total += defs.len();

        // invariant 2: real-parser rejection is never silent
        if let Some(bin) = &bin {
            let out = std::process::Command::new(bin)
                .args(["-C", "-f"])
                .arg(f)
                .output()
                .expect("spawn opensips -C");
            let rc = out.status.code().unwrap_or(-1);
            let all = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let diags = diag::parse_check_output(&all, rc);
            if rc == 0 {
                ok += 1;
                assert!(
                    diags.is_empty(),
                    "{}: accepted by -C but diagnostics produced: {diags:?}",
                    f.display()
                );
            } else {
                broken += 1;
                assert!(
                    !diags.is_empty(),
                    "{}: rejected by -C (rc={rc}) but NO diagnostic surfaced — silent failure.\noutput:\n{}",
                    f.display(),
                    &all[..all.len().min(2000)]
                );
            }
        }
    }
    eprintln!(
        "corpus: {} files, {} loadmodules, {} route blocks{}",
        files.len(),
        mods_total,
        routes_total,
        if bin.is_some() {
            format!(", -C: {ok} accepted / {broken} rejected (older examples are known-broken)")
        } else {
            String::new()
        }
    );
}
