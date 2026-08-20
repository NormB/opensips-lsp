//! Workspace symbols and codeLens reference counts over stdio.

mod common;
use common::*;
use std::process::{Command, Stdio};

const DOC: &str = "route[helper] {\n    exit;\n}\nroute[other_thing] {\n    exit;\n}\nroute {\n    route(helper);\n    route(helper);\n}\n";

fn boot(
    tag: &str,
    opts: serde_json::Value,
    text: &str,
) -> (
    std::process::Child,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("oslsp-nav-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("t.cfg");
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
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{}, "initializationOptions": opts}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    assert!(
        init["result"]["capabilities"]["workspaceSymbolProvider"]
            .as_bool()
            .unwrap_or(false),
        "workspaceSymbolProvider must be advertised"
    );
    assert!(
        !init["result"]["capabilities"]["codeLensProvider"].is_null(),
        "codeLensProvider must be advertised"
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
    (child, rx, stdin, uri, base)
}

#[test]
fn workspace_symbols_filter_across_open_docs() {
    let (mut child, rx, mut stdin, _uri, base) =
        boot("ws", serde_json::json!({"opensipsPath":""}), DOC);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"workspace/symbol","params":{"query":"help"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "wsym");
    let syms = v["result"].as_array().expect("symbols");
    assert_eq!(syms.len(), 1, "{syms:?}");
    assert_eq!(syms[0]["name"], "route[helper]");
    // case-insensitive; empty query returns everything named
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"workspace/symbol","params":{"query":"HELP"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "wsym-ci");
    assert_eq!(v["result"].as_array().unwrap().len(), 1);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"workspace/symbol","params":{"query":""}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "wsym-all");
    assert!(v["result"].as_array().unwrap().len() >= 2);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn code_lens_counts_route_references() {
    let (mut child, rx, mut stdin, uri, base) =
        boot("cl", serde_json::json!({"opensipsPath":""}), DOC);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeLens","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codelens");
    let lenses = v["result"].as_array().expect("lenses");
    // named request routes only (the anonymous main route has no lens)
    assert_eq!(lenses.len(), 2, "{lenses:?}");
    let helper = lenses
        .iter()
        .find(|l| l["range"]["start"]["line"] == 0)
        .expect("lens on helper");
    assert_eq!(helper["command"]["title"], "2 references");
    let other = lenses
        .iter()
        .find(|l| l["range"]["start"]["line"] == 3)
        .expect("lens on other_thing");
    assert_eq!(other["command"]["title"], "0 references");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn code_lens_can_be_disabled() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "cloff",
        serde_json::json!({"opensipsPath":"", "codeLensReferences": false}),
        DOC,
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeLens","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codelens-off");
    assert!(
        v["result"].is_null() || v["result"].as_array().unwrap().is_empty(),
        "no lenses when disabled: {v}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn quick_fix_creates_a_route_stub() {
    let broken = "route {\n    route(missing);\n}\n";
    let (mut child, rx, mut stdin, uri, base) =
        boot("qf", serde_json::json!({"opensipsPath":""}), broken);
    // ask for actions carrying the analyzer diagnostic in context
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/codeAction","params":{
        "textDocument":{"uri":uri},
        "range":{"start":{"line":1,"character":10},"end":{"line":1,"character":17}},
        "context":{"diagnostics":[{
            "range":{"start":{"line":1,"character":10},"end":{"line":1,"character":17}},
            "severity":2,"source":"opensips-lsp",
            "message":"route 'missing' is not defined here or in included files"}]}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "codeAction");
    let actions = v["result"].as_array().expect("actions");
    assert_eq!(actions.len(), 1, "{actions:?}");
    let a = &actions[0];
    assert_eq!(a["title"], "Create route[missing]");
    assert_eq!(a["kind"], "quickfix");
    let edits = a["edit"]["changes"][&uri].as_array().expect("edit");
    assert_eq!(edits[0]["range"]["start"]["line"], 3, "appended at EOF");
    assert!(
        edits[0]["newText"]
            .as_str()
            .unwrap()
            .contains("route[missing]")
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn semantic_tokens_over_stdio() {
    let (mut child, rx, mut stdin, uri, base) = boot(
        "sem",
        serde_json::json!({"opensipsPath":""}),
        "route[ab] {\n    route(ab);\n}\n",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/semanticTokens/full","params":{
        "textDocument":{"uri":uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "semanticTokens");
    let data = v["result"]["data"].as_array().expect("data");
    assert_eq!(data.len() % 5, 0);
    let first: Vec<u64> = data[..5].iter().map(|x| x.as_u64().unwrap()).collect();
    assert_eq!(first, vec![0, 6, 2, 0, 0], "route name at line 0 col 6");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
