//! Server-side settings introduced with the snippets/settings round:
//!   - snippetCompletions: function completions insert tabstop
//!     snippets (default on; false = plain text)
//!   - maxDiagnostics: bound on published diagnostics per file
//!   - cacheDir: catalog cache location as an initialization option

mod common;
use common::*;
use std::process::{Command, Stdio};

fn mk_tree(root: &std::path::Path) {
    let w = |p: std::path::PathBuf, c: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    };
    w(
        root.join("modules/mymod/README.md"),
        "# mymod\n\n### Exported Functions\n\n#### my_func(arg)\n\nDoes things.\n",
    );
}

fn boot(
    tag: &str,
    extra_opts: serde_json::Value,
    stub_errors: usize,
) -> (
    std::process::Child,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("oslsp-set-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    mk_tree(&tree);
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, "loadmodule \"mymod.so\"\nroute { exit; }\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let stub = base.join("stub.sh");
    let mut lines = String::from("#!/bin/sh\nfor a in \"$@\"; do cfg=\"$a\"; done\n");
    for i in 0..stub_errors {
        lines.push_str(&format!(
            "echo \"CRITICAL:core:yyerror: parse error in $cfg:1:{}-{}: planted {i}\" >&2\n",
            i + 1,
            i + 2
        ));
    }
    lines.push_str("exit 255\n");
    std::fs::write(&stub, lines).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let mut opts = serde_json::json!({
        "opensipsPath": if stub_errors > 0 { stub.display().to_string() } else { String::new() },
        "opensipsSrc": tree.display().to_string(),
        "cacheDir": base.join("cache").display().to_string(),
    });
    for (k, v) in extra_opts.as_object().unwrap() {
        opts[k] = v.clone();
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env_remove("OPENSIPS_LSP_BIN")
        .env_remove("OPENSIPS_LSP_SRC")
        .env_remove("OPENSIPS_LSP_CACHE_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{}, "initializationOptions": opts}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,
                "text":"loadmodule \"mymod.so\"\nroute { exit; }\n"}}}),
    );
    (child, rx, stdin, uri, base)
}

fn completion_item<'a>(v: &'a serde_json::Value, label: &str) -> Option<&'a serde_json::Value> {
    v["result"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == label)
}

#[test]
fn function_completions_are_snippets_by_default() {
    let (mut child, rx, mut stdin, uri, base) = boot("snip", serde_json::json!({}), 0);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":8}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "completion");
    let item = completion_item(&v, "my_func").expect("my_func offered");
    assert_eq!(item["insertTextFormat"], 2, "snippet format: {item}");
    assert_eq!(
        item["insertText"], "my_func($1)$0",
        "tabstop snippet: {item}"
    );
    // keywords stay plain
    let kw = completion_item(&v, "exit").expect("keyword");
    assert!(kw.get("insertText").is_none(), "keywords stay plain: {kw}");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn snippet_completions_can_be_disabled() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "nosnip",
        serde_json::json!({"snippetCompletions": false}),
        0,
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":8}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "completion");
    let item = completion_item(&v, "my_func").expect("my_func offered");
    assert!(
        item.get("insertTextFormat").is_none() && item.get("insertText").is_none(),
        "plain completion when disabled: {item}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn max_diagnostics_bounds_the_publish() {
    let (mut child, rx, _stdin, _uri, base) =
        boot("maxdiag", serde_json::json!({"maxDiagnostics": 2}), 5);
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().unwrap().len(),
        2,
        "5 planted, capped at 2: {d}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn cache_dir_option_is_honored() {
    let (mut child, _rx, _stdin, _uri, base) = boot("cachedir", serde_json::json!({}), 0);
    // the harvest above must have written the cache into the OPTION path
    let cache = base.join("cache");
    let n = std::fs::read_dir(&cache).map(|d| d.count()).unwrap_or(0);
    assert!(n >= 1, "cacheDir option ignored: {} entries", n);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn harvest_reports_progress_and_warns_on_an_empty_tree() {
    let base = std::env::temp_dir().join(format!("oslsp-set-prog-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // a CONFIGURED tree that yields no documentation at all
    let tree = base.join("empty-tree");
    std::fs::create_dir_all(tree.join("modules")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", "")
        .env(
            "OPENSIPS_LSP_CACHE_DIR",
            base.join("cache").display().to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{"window":{"workDoneProgress":true}},
        "initializationOptions":{"opensipsSrc": tree.display().to_string()}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // the server asks to create a progress token; answer it
    let create = wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create",
    );
    let token = create["params"]["token"].clone();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":create["id"],"result":null}),
    );
    // begin ... end on that token
    let begin = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "begin",
        "progress begin",
    );
    assert_eq!(begin["params"]["token"], token);
    assert!(
        begin["params"]["value"]["title"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("harvest"),
        "{begin}"
    );
    let end = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "end",
        "progress end",
    );
    assert_eq!(end["params"]["token"], token);
    // an explicitly configured tree with zero symbols warns the user
    let warn = wait_for(
        &rx,
        |v| v["method"] == "window/showMessage" && v["params"]["type"] == 2,
        "zero-symbol warning",
    );
    let m = warn["params"]["message"].as_str().unwrap();
    assert!(
        m.contains("empty-tree") && m.to_lowercase().contains("no module"),
        "warning names the path and the problem: {m}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn runtime_settings_apply_without_a_server_restart() {
    let base = std::env::temp_dir().join(format!("oslsp-set-dyn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("t.cfg");
    let text = "route {\n    route(a);\n    route(b);\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
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
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "analyzer diags",
    );
    assert_eq!(
        v["params"]["diagnostics"].as_array().unwrap().len(),
        2,
        "two undefined routes: {v}"
    );
    // flat settings shape: analyzer off → open docs republished clean
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
        "settings":{"analyzerDiagnostics":false}}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "cleared after toggle-off",
    );
    assert_eq!(v["params"]["uri"].as_str().unwrap(), uri);
    // nested (VS Code) shape: analyzer back on, capped to 1
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
        "settings":{"opensipsLsp":{"analyzerDiagnostics":true,"maxDiagnostics":1}}}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"].as_array().unwrap().len() == 1
        },
        "capped republish",
    );
    assert!(
        v["params"]["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("not defined"),
        "{v}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
