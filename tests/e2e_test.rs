//! End-to-end: spawn the real server binary and speak LSP over stdio.
//! A stub `opensips` binary supplies deterministic -C output.

mod common;
use common::Server;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn diagnostics_flow_over_stdio() {
    let dir_scratch = common::Scratch::new("oslsp-e2e");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"capabilities":{}}}),
    );
    let init = common::wait_for(&rx, |v| v["id"] == 1, "initialize result");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(false),
        "the server must advertise hoverProvider: {init}"
    );

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,
                "text":"loadmodule \"nope.so\"\nbroken here\n"}}}),
    );

    let diag = common::wait_for(
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

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"shutdown"}),
    );
    common::wait_for(&rx, |v| v["id"] == 9, "shutdown result");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"exit"}),
    );
    // real clients close the pipe after `exit`; tower-lsp-server's serve loop
    // terminates on stdin EOF
    drop(stdin);
    // bounded wait: a server that ignores `exit` must fail the test,
    // not hang it
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(st) = child.try_wait().expect("try_wait") {
            break st;
        }
        if Instant::now() > deadline {
            child.kill().ok();
            panic!("server did not exit within 10s of the exit notification");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "clean exit, got {status:?}");
}

#[test]
fn hanging_opensips_check_is_bounded_and_reported() {
    let dir_scratch = common::Scratch::new("oslsp-e2e-hang");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "initialize result");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );

    // within a couple seconds (NOT 60) we must hear the timeout warning
    let log = common::wait_for(
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
            .contains("timed out"),
        "the check must report its timeout via logMessage: {log}"
    );
    child.kill().ok();
}

#[test]
fn empty_opensips_bin_disables_checks_entirely() {
    let dir_scratch = common::Scratch::new("oslsp-e2e-off");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "initialize result");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display()),"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );
    // request hover to force a full round-trip; NO publishDiagnostics
    // may arrive before its response
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
        "textDocument":{"uri":format!("file://{}", cfg.display())},"position":{"line":0,"character":1}}}),
    );
    let mut saw_diags = false;
    loop {
        let v = common::wait_for(&rx, |_| true, "hover response");
        if v["method"] == "textDocument/publishDiagnostics" {
            saw_diags = true;
        }
        if v["id"] == 2 {
            break;
        }
    }
    assert!(!saw_diags, "diagnostics must be fully disabled");
    child.kill().ok();
}

#[test]
fn symbol_columns_are_utf16_on_multibyte_lines() {
    let dir_scratch = common::Scratch::new("oslsp-utf16");
    let dir = dir_scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("u.cfg");
    // the comment holds é (2 bytes / 1 unit) and 😀 (4 bytes / 2 units):
    // `route` starts at byte 13 / UTF-16 10, its name `x` at byte 19 /
    // UTF-16 16
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "symbols");
    let sym = &v["result"][0];
    assert_eq!(
        sym["range"]["start"]["character"], 10,
        "must be the UTF-16 column, not the byte column (13)"
    );
    assert_eq!(
        sym["selectionRange"]["start"]["character"], 16,
        "the name, in UTF-16, not the byte column (19)"
    );
    assert_eq!(sym["selectionRange"]["end"]["character"], 17);
    child.kill().ok();
}

#[test]
fn folding_and_nested_symbols_over_stdio() {
    let dir_scratch = common::Scratch::new("oslsp-fold");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = common::wait_for(&rx, |v| v["id"] == 1, "init");
    assert!(
        init["result"]["capabilities"]["foldingRangeProvider"]
            .as_bool()
            .unwrap_or(false),
        "foldingRangeProvider must be advertised"
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/foldingRange","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "folding");
    let folds = v["result"].as_array().expect("folding array");
    let got: Vec<(u64, u64)> = folds
        .iter()
        .map(|f| {
            (
                f["startLine"].as_u64().unwrap(),
                f["endLine"].as_u64().unwrap(),
            )
        })
        .collect();
    // both route blocks and the `if` inside the first; the `}` in the
    // string closes nothing
    assert_eq!(got, vec![(1, 5), (2, 4), (6, 8)], "{folds:?}");

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/documentSymbol","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 3, "symbols");
    let syms = v["result"].as_array().expect("symbol array");
    let names: Vec<&str> = syms.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["tm", "route (main)", "failure_route[fr]"]);
    // nested DocumentSymbol shape: full block range + selectionRange
    let main = &syms[1];
    assert_eq!(main["range"]["start"]["line"], 1);
    assert_eq!(main["range"]["end"]["line"], 5);
    assert_eq!(main["range"]["end"]["character"], 1);
    assert_eq!(main["selectionRange"]["start"]["line"], 1);
    // the block inside the route nests under it
    let inner = main["children"].as_array().expect("children");
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0]["name"], "if (1)");
    assert_eq!(inner[0]["range"]["start"]["line"], 2);
    assert_eq!(inner[0]["range"]["end"]["line"], 4);
    assert_eq!(syms[2]["range"]["end"]["line"], 8);
    assert_eq!(syms[2]["selectionRange"]["start"]["character"], 14);
    child.kill().ok();
}

#[test]
fn references_rename_highlight_over_stdio() {
    let dir_scratch = common::Scratch::new("oslsp-refs");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = common::wait_for(&rx, |v| v["id"] == 1, "init");
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
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // references from a call site, declaration included → 2 refs + def
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":true}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "references");
    let locs = v["result"].as_array().expect("locations");
    assert_eq!(locs.len(), 3, "2 refs + 1 def: {locs:?}");
    // declaration excluded → 2
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},
        "context":{"includeDeclaration":false}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 3, "references-nodecl");
    assert_eq!(v["result"].as_array().unwrap().len(), 2);
    // highlights at the def name: 3 occurrences, def is Write(3)
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/documentHighlight","params":{
        "textDocument":{"uri":uri},"position":{"line":4,"character":7}}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 4, "highlight");
    let hls = v["result"].as_array().expect("highlights");
    assert_eq!(hls.len(), 3);
    assert!(
        hls.iter().any(|h| h["kind"] == 3),
        "def highlighted as Write"
    );
    // rename to a valid name rewrites every occurrence
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":"fwd_1"}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 5, "rename");
    let edits = v["result"]["changes"][&uri].as_array().expect("edits");
    assert_eq!(edits.len(), 3);
    assert!(
        edits.iter().all(|e| e["newText"] == "fwd_1"),
        "every rename edit must write the new name fwd_1: {edits:?}"
    );
    // the quoted ref's edit replaces only the name inside the quotes
    let quoted = edits
        .iter()
        .find(|e| e["range"]["start"]["line"] == 2)
        .unwrap();
    assert_eq!(quoted["range"]["start"]["character"], 11);
    assert_eq!(quoted["range"]["end"]["character"], 16);
    // rename to an ILLEGAL name is rejected with an error
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":6,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":1,"character":11},"newName":"has space"}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 6, "rename-bad");
    assert!(
        !v["error"].is_null(),
        "illegal new name must be a jsonrpc error: {v}"
    );
    child.kill().ok();
}

#[test]
fn check_invocation_carries_the_opensips_flags() {
    let dir_scratch = common::Scratch::new("oslsp-argv");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route { exit; }\n"}}}),
    );
    // rc=0 with clean output publishes empty diagnostics; wait for it
    common::wait_for(
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
}

#[test]
fn check_runs_in_the_configs_own_directory() {
    // relative include_file / module paths resolve against the
    // checker's cwd — the server must run it from the cfg's dir,
    // exactly like the CLI does
    let dir_scratch = common::Scratch::new("oslsp-cwd");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route { exit; }\n"}}}),
    );
    common::wait_for(
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
}

#[test]
fn a_newer_save_supersedes_a_slow_running_check() {
    // latest-wins: the running -C for stale content is killed the
    // moment a newer check for the same document arrives — the fresh
    // result must not wait out the slow run (nor its timeout)
    let dir_scratch = common::Scratch::new("oslsp-super");
    let dir = dir_scratch.path().to_path_buf();
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let t0 = Instant::now();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":v1}}}),
    );
    // give the slow check time to actually spawn and hold the gate
    std::thread::sleep(Duration::from_millis(400));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":v2}]}}),
    );
    // the editor saves the new content to disk, then notifies
    std::fs::write(&cfg, v2).unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
        "textDocument":{"uri":uri}}}),
    );
    // the -C result specifically (the debounced analyzer also
    // publishes version 2, but with no checker diagnostics)
    let v = common::wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["version"] == 2
                && !v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "fresh -C result",
    );
    assert!(
        t0.elapsed() < Duration::from_secs(10),
        "the superseded run must be killed, not awaited"
    );
    let msgs = v["params"]["diagnostics"].to_string();
    assert!(msgs.contains("fresh"), "newest run's diagnostic: {msgs}");
    assert!(!msgs.contains("slow"), "killed run must not leak: {msgs}");
    child.kill().ok();
}

#[test]
fn selection_range_grows_outward_over_stdio() {
    let dir_scratch = common::Scratch::new("oslsp-selrange");
    let dir = dir_scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("s.cfg");
    // `é` is two bytes and one UTF-16 unit, so every column on line 1
    // after it differs between the two
    let text = "route {\n    if ($rU == \"é\") { xlog(\"x\"); }\n}\n";
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
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    let init = common::wait_for(&rx, |v| v["id"] == 1, "init");
    assert_eq!(
        init["result"]["capabilities"]["selectionRangeProvider"],
        serde_json::json!(true)
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // `xlog` is UTF-16 columns 22..26 and bytes 23..27; the cursor
    // sits inside it
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/selectionRange","params":{
        "textDocument":{"uri":uri},"positions":[{"line":1,"character":24}]}}),
    );
    let v = common::wait_for(&rx, |v| v["id"] == 2, "selectionRange");
    let mut chain = Vec::new();
    let mut cur = &v["result"][0];
    while !cur.is_null() {
        let r = &cur["range"];
        chain.push(format!(
            "{}:{}-{}:{}",
            r["start"]["line"], r["start"]["character"], r["end"]["line"], r["end"]["character"]
        ));
        cur = &cur["parent"];
    }
    assert_eq!(
        chain,
        vec![
            "1:22-1:26", // xlog
            "1:4-1:34",  // the if statement
            "0:0-2:1",   // the route block
            "0:0-3:0",   // the document
        ]
    );
    child.kill().ok();
}
