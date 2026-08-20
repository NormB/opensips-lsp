//! The `opensips-lsp check` CLI mode: the same analyzer (plus the
//! real `-C` when a binary is configured) as a CI/git-hook command.

use std::process::Command;

fn setup(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oslsp-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_reports_analyzer_warnings_and_exits_zero() {
    let dir = setup("warn");
    let cfg = dir.join("w.cfg");
    std::fs::write(&cfg, "route {\n    route(missing);\n}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("OPENSIPS_LSP_BIN", "")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // 1-based positions for humans, grep-able shape
    assert!(
        stdout.contains("w.cfg:2:11: warning: route 'missing' is not defined"),
        "got: {stdout}"
    );
    assert_eq!(out.status.code(), Some(0), "warnings alone exit 0");
    // --strict promotes warnings to failure
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args(["check", "--strict", cfg.to_str().unwrap()])
        .env("OPENSIPS_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "--strict fails on warnings");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_runs_the_configured_binary_and_fails_on_errors() {
    let dir = setup("bin");
    let cfg = dir.join("b.cfg");
    std::fs::write(&cfg, "route {\n    exit;\n}\n").unwrap();
    // stub checker: one positioned error, rc=255
    let stub = dir.join("stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\nfor a in \"$@\"; do cfg=\"$a\"; done\necho \"CRITICAL:core:yyerror: parse error in $cfg:2:5-9: planted error\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args([
            "check",
            "--bin",
            stub.to_str().unwrap(),
            cfg.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("b.cfg:2:5: error: planted error"),
        "got: {stdout}"
    );
    assert_eq!(out.status.code(), Some(1), "errors exit 1");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_clean_file_exits_zero_and_bad_usage_exits_two() {
    let dir = setup("clean");
    let cfg = dir.join("c.cfg");
    std::fs::write(
        &cfg,
        "route[a] {\n    exit;\n}\nroute {\n    route(a);\n}\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args(["check", cfg.to_str().unwrap()])
        .env("OPENSIPS_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    // missing file → usage-style failure, not a panic
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args(["check", "/nonexistent/x.cfg"])
        .env("OPENSIPS_LSP_BIN", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "unreadable file exits 2");
    // no files at all
    let out = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .args(["check"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let _ = std::fs::remove_dir_all(&dir);
}
