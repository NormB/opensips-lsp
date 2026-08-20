//! Differential gates against the REAL parser (the ground truth):
//!
//!   - the analyzer must be SILENT on every corpus config the binary
//!     accepts — any false positive on real-world configs is a CI
//!     failure, not an audit finding;
//!   - a rename applied through the real logic path must produce a
//!     config the binary still accepts.
//!
//! Both are env-gated: OPENSIPS_LSP_TEST_TREE / OPENSIPS_LSP_TEST_BIN.

use opensips_lsp::logic;

/// Same enumeration as the corpus sweep: shipped example configs.
fn corpus(tree: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for dir in [tree.join("etc"), tree.join("examples")] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "cfg") {
                out.push(p);
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(tree.join("modules")) {
        for e in rd.flatten() {
            for sub in ["examples", "etc"] {
                let d = e.path().join(sub);
                let Ok(rd2) = std::fs::read_dir(&d) else {
                    continue;
                };
                for f in rd2.flatten() {
                    let p = f.path();
                    if p.extension().is_some_and(|x| x == "cfg") {
                        out.push(p);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn disk_loader(p: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > 1_048_576 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

#[test]
fn analyzer_is_silent_on_binary_accepted_configs() {
    let (Ok(tree), Ok(bin)) = (
        std::env::var("OPENSIPS_LSP_TEST_TREE"),
        std::env::var("OPENSIPS_LSP_TEST_BIN"),
    ) else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_TREE / OPENSIPS_LSP_TEST_BIN not set");
        return;
    };
    let mut files = corpus(std::path::Path::new(&tree));
    assert!(!files.is_empty(), "no corpus configs under {tree}");
    // the shipped examples may all be rejected on a box with a partial
    // module install; a synthetic baseline (verified-accepted
    // constructs, including past false-positive shapes: route(0),
    // include-defined targets, numeric routes) keeps the gate
    // non-vacuous everywhere
    let base = std::env::temp_dir().join(format!("oslsp-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        base.join("inc.cfg"),
        "route[from_include] {\n    exit;\n}\n",
    )
    .unwrap();
    for (name, text) in [
        (
            "baseline-basic.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[named] {\n    exit;\n}\nroute[42] {\n    exit;\n}\nroute {\n    route(named);\n    route(42);\n    route(0);\n    exit;\n}\n",
        ),
        (
            "baseline-include.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\ninclude_file \"inc.cfg\"\nroute {\n    route(from_include);\n    exit;\n}\n",
        ),
    ] {
        let p = base.join(name);
        std::fs::write(&p, text).unwrap();
        files.push(p);
    }
    let mut accepted = 0usize;
    for f in &files {
        let Ok(out) = std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(f)
            .current_dir(f.parent().unwrap())
            .output()
        else {
            continue;
        };
        if out.status.code() != Some(0) {
            continue; // rejected configs are the corpus sweep's business
        }
        accepted += 1;
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let warns = logic::analyzer_diagnostics(f, &text, &disk_loader);
        assert!(
            warns.is_empty(),
            "{}: accepted by the real parser but the analyzer warns — \
             false positive:\n{warns:#?}",
            f.display()
        );
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(accepted > 0, "no config was accepted; gate is vacuous");
    eprintln!("analyzer silent on {accepted} binary-accepted configs");
}

#[test]
fn rename_round_trip_survives_the_real_parser() {
    let Ok(bin) = std::env::var("OPENSIPS_LSP_TEST_BIN") else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_BIN not set");
        return;
    };
    let dir = std::env::temp_dir().join(format!("oslsp-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("rt.cfg");
    let original = "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[old_target] {\n    exit;\n}\nroute {\n    route(old_target);\n    exit;\n}\n";
    std::fs::write(&cfg, original).unwrap();
    let check = |p: &std::path::Path| {
        std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(p)
            .current_dir(p.parent().unwrap())
            .output()
            .map(|o| o.status.code() == Some(0))
            .unwrap_or(false)
    };
    if !check(&cfg) {
        eprintln!("SKIP: this binary rejects the baseline config");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    // rename through the REAL logic path: gate + occurrences
    let new_name = "relay_v2";
    assert!(logic::valid_route_name(new_name));
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let mut occ = logic::route_occurrences(original, "old_target");
    assert_eq!(occ.len(), 2, "def + one call site");
    // splice right-to-left so earlier columns stay valid
    occ.sort_by_key(|(l, _)| (l.line, std::cmp::Reverse(l.col)));
    for (l, _) in &occ {
        let line = &mut lines[l.line as usize];
        let (s, e) = (l.col as usize, l.col as usize + l.name.len());
        line.replace_range(s..e, new_name);
    }
    let renamed = lines.join("\n") + "\n";
    assert!(!renamed.contains("old_target"));
    assert!(renamed.matches(new_name).count() == 2);
    let out_cfg = dir.join("rt-renamed.cfg");
    std::fs::write(&out_cfg, &renamed).unwrap();
    assert!(
        check(&out_cfg),
        "the real parser rejects the renamed config:\n{renamed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
