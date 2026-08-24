//! End-to-end: spawn the real server binary and speak LSP over stdio.
//! A stub `opensips` binary supplies deterministic -C output.

mod common;
use common::*;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn diagnostics_flow_over_stdio() {
    let dir = std::env::temp_dir().join(format!("oslsp-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // deterministic stub standing in for the opensips binary
    let stub = dir.join("opensips-stub.sh");
    std::fs::write(
        &stub,
        "#!/bin/sh\n# args: -C -f <cfg>\necho \"CRITICAL:core:yyerror: parse error in $3:2:5-7: stub says no\" >&2\nexit 255\n",
    )
    .unwrap();
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let cfg = dir.join("test.cfg");
    std::fs::write(&cfg, "loadmodule \"nope.so\"\nbroken here\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_SRC", "") // no catalog needed for this flow
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("server binary must spawn"),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize result");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false)
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,
                "text":"loadmodule \"nope.so\"\nbroken here\n"}}}),
    );

    let diag = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publishDiagnostics",
    );
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(ds.len(), 1, "exactly the stub diagnostic: {ds:?}");
    assert_eq!(ds[0]["message"], "stub says no");
    assert_eq!(ds[0]["range"]["start"]["line"], 1);
    assert_eq!(ds[0]["range"]["start"]["character"], 4);
    assert_eq!(ds[0]["range"]["end"]["character"], 6);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}),
    );
    wait_for(&rx, |v| v["id"] == 9, "shutdown result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
    );
    // real clients close the pipe after `exit`; tower-lsp-server's serve loop
    // terminates on stdin EOF
    drop(stdin);
    // bounded wait: a server that ignores `exit` must fail the test,
    // not hang it
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(st) = child.try_wait().expect("try_wait") {
            break st;
        }
        if std::time::Instant::now() > deadline {
            child.kill().ok();
            panic!("server did not exit within 10s of the exit notification");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "clean exit, got {status:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hanging_opensips_check_is_bounded_and_reported() {
    let dir = std::env::temp_dir().join(format!("oslsp-e2e-hang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("opensips-hang.sh");
    std::fs::write(&stub, "#!/bin/sh\nsleep 60\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "route{}\n").unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "300")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );

    // within a couple seconds (NOT 60) we must hear the timeout warning
    let log = wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("timed out")
        },
        "check-timeout logMessage",
    );
    assert!(
        log["params"]["message"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_opensips_bin_disables_checks_entirely() {
    let dir = std::env::temp_dir().join(format!("oslsp-e2e-off-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "route{}\n").unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "") // explicit opt-out
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize result");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );
    // request hover to force a full round-trip; NO publishDiagnostics
    // may arrive before its response
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display())},"position":{"line":0,"character":1}}}),
    );
    let mut saw_diags = false;
    loop {
        let v = wait_for(&rx, |_| true, "hover response");
        if v["method"] == "textDocument/publishDiagnostics" {
            saw_diags = true;
        }
        if v["id"] == 2 {
            break;
        }
    }
    assert!(!saw_diags, "diagnostics must be fully disabled");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn symbol_columns_are_utf16_on_multibyte_lines() {
    let dir = std::env::temp_dir().join(format!("oslsp-utf16-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("u.cfg");
    // the comment holds é (2 bytes / 1 unit) and 😀 (4 bytes / 2 units):
    // byte col of `route` = 12, utf16 col = 9
    let text = "/* \u{e9}\u{1F600} */ route[x] { exit; }\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "symbols");
    let col = v["result"][0]["selectionRange"]["start"]["character"]
        .as_u64()
        .unwrap();
    assert_eq!(
        col, 10,
        "must be the UTF-16 column, not the byte column (13)"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn folding_and_nested_symbols_over_stdio() {
    let dir = std::env::temp_dir().join(format!("oslsp-fold-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("fold.cfg");
    let text = "loadmodule \"tm.so\"\nroute {\n    if (1) {\n        exit;\n    }\n}\nfailure_route[fr] {\n    xlog(\"}\");\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    assert!(
        init["result"]["capabilities"]["foldingRangeProvider"]
            .as_bool()
            .unwrap_or(false),
        "foldingRangeProvider must be advertised"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/foldingRange","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "folding");
    let folds = v["result"].as_array().expect("folding array");
    assert_eq!(folds.len(), 2, "one fold per route block: {folds:?}");
    assert_eq!(folds[0]["startLine"], 1);
    assert_eq!(folds[0]["endLine"], 5);
    assert_eq!(folds[1]["startLine"], 6);
    assert_eq!(folds[1]["endLine"], 8);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "symbols");
    let syms = v["result"].as_array().expect("symbol array");
    assert_eq!(syms.len(), 2);
    // nested DocumentSymbol shape: full block range + selectionRange
    assert_eq!(syms[0]["name"], "route (main)");
    assert_eq!(syms[0]["range"]["start"]["line"], 1);
    assert_eq!(syms[0]["range"]["end"]["line"], 5);
    assert_eq!(syms[0]["range"]["end"]["character"], 1);
    assert_eq!(syms[0]["selectionRange"]["start"]["line"], 1);
    assert_eq!(syms[1]["name"], "failure_route[fr]");
    assert_eq!(syms[1]["range"]["end"]["line"], 8);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn references_rename_highlight_over_stdio() {
    let dir = std::env::temp_dir().join(format!("oslsp-refs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("refs.cfg");
    let text =
        "route {\n    route(relay);\n    route(\"relay\");\n}\nroute[relay] {\n    exit;\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    for cap in ["referencesProvider", "documentHighlightProvider"] {
        assert!(
            init["result"]["capabilities"][cap]
                .as_bool()
                .unwrap_or(false),
            "{cap} must be advertised"
        );
    }
    assert!(
        !init["result"]["capabilities"]["renameProvider"].is_null(),
        "renameProvider must be advertised"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // references from a call site, declaration included → 2 refs + def
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":true}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "references");
    let locs = v["result"].as_array().expect("locations");
    assert_eq!(locs.len(), 3, "2 refs + 1 def: {locs:?}");
    // declaration excluded → 2
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":false}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "references-nodecl");
    assert_eq!(v["result"].as_array().unwrap().len(), 2);
    // highlights at the def name: 3 occurrences, def is Write(3)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/documentHighlight","params":{
        "textDocument":{"uri":uri},"position":{"line":4,"character":7}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "highlight");
    let hls = v["result"].as_array().expect("highlights");
    assert_eq!(hls.len(), 3);
    assert!(
        hls.iter().any(|h| h["kind"] == 3),
        "def highlighted as Write"
    );
    // rename to a valid name rewrites every occurrence
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":"fwd_1"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 5, "rename");
    let edits = v["result"]["changes"][&uri].as_array().expect("edits");
    assert_eq!(edits.len(), 3);
    assert!(edits.iter().all(|e| e["newText"] == "fwd_1"));
    // the quoted ref's edit replaces only the name inside the quotes
    let quoted = edits
        .iter()
        .find(|e| e["range"]["start"]["line"] == 2)
        .unwrap();
    assert_eq!(quoted["range"]["start"]["character"], 11);
    assert_eq!(quoted["range"]["end"]["character"], 16);
    // rename to an ILLEGAL name is rejected with an error
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":6,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":"has space"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 6, "rename-bad");
    assert!(
        !v["error"].is_null(),
        "illegal new name must be a jsonrpc error: {v}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_invocation_carries_the_opensips_flags() {
    let dir = std::env::temp_dir().join(format!("oslsp-argv-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // recording stub: append its argv to a file, one arg per line
    let log = dir.join("argv.log");
    let stub = dir.join("rec.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\" >> {}; done\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "route { exit; }\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route { exit; }\n"}}}),
    );
    // rc=0 with clean output publishes empty diagnostics; wait for it
    wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish after check",
    );
    let argv = std::fs::read_to_string(&log).expect("stub ran");
    let args: Vec<&str> = argv.lines().collect();
    assert_eq!(
        args,
        vec!["-C", "-f", cfg.to_str().unwrap()],
        "exact opensips check argv"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_runs_in_the_configs_own_directory() {
    // relative include_file / module paths resolve against the
    // checker's cwd — the server must run it from the cfg's dir,
    // exactly like the CLI does
    let dir = std::env::temp_dir().join(format!("oslsp-cwd-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let conf = dir.join("conf");
    std::fs::create_dir_all(&conf).unwrap();
    let log = dir.join("pwd.log");
    let stub = dir.join("rec-pwd.sh");
    std::fs::write(
        &stub,
        format!("#!/bin/sh\npwd >> {}\nexit 0\n", log.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = conf.join("t.cfg");
    std::fs::write(&cfg, "route { exit; }\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route { exit; }\n"}}}),
    );
    wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish after check",
    );
    let pwd = std::fs::read_to_string(&log).expect("stub ran");
    // canonicalize both sides: macOS tempdirs live under /private
    let got = std::fs::canonicalize(pwd.trim()).unwrap();
    let want = std::fs::canonicalize(&conf).unwrap();
    assert_eq!(got, want, "checker must run from the cfg's directory");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_newer_save_supersedes_a_slow_running_check() {
    // latest-wins: the running -C for stale content is killed the
    // moment a newer check for the same document arrives — the fresh
    // result must not wait out the slow run (nor its timeout)
    let dir = std::env::temp_dir().join(format!("oslsp-super-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("slowfast.sh");
    // SLOW content: hang far past every test budget; FAST: clean exit
    std::fs::write(
        &stub,
        "#!/bin/sh\nif grep -q SLOW \"$3\"; then sleep 30; echo \"CRITICAL:core:yyerror: parse error in $3:1:1-2: slow\" >&2; exit 255; fi\necho \"CRITICAL:core:yyerror: parse error in $3:1:1-2: fresh\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();
    let cfg = dir.join("t.cfg");
    let v1 = "# SLOW\nroute { exit; }\n";
    let v2 = "route { exit; }\n";
    std::fs::write(&cfg, v1).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "60000")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let t0 = std::time::Instant::now();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":v1}}}),
    );
    // give the slow check time to actually spawn and hold the gate
    std::thread::sleep(std::time::Duration::from_millis(400));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":v2}]}}),
    );
    // the editor saves the new content to disk, then notifies
    std::fs::write(&cfg, v2).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
        "textDocument":{"uri":uri}}}),
    );
    // the -C result specifically (the debounced analyzer also
    // publishes version 2, but with no checker diagnostics)
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["version"] == 2
                && !v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "fresh -C result",
    );
    assert!(
        t0.elapsed() < std::time::Duration::from_secs(10),
        "the superseded run must be killed, not awaited"
    );
    let msgs = v["params"]["diagnostics"].to_string();
    assert!(msgs.contains("fresh"), "newest run's diagnostic: {msgs}");
    assert!(!msgs.contains("slow"), "killed run must not leak: {msgs}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
