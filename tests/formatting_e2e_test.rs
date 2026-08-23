//! `textDocument/formatting` and `textDocument/rangeFormatting` over
//! real LSP stdio: the capability must be advertised, the edits must
//! be whole-line replacements that leave newlines alone, and a
//! document that is already correct must produce no edits at all.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn boot(
    tag: &str,
    text: &str,
) -> (
    std::process::Child,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let dir = std::env::temp_dir().join(format!("oslsp-fmt-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", "") // diagnostics off: this is about edits
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
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize");
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        serde_json::json!(true),
        "formatting capability not advertised"
    );
    assert_eq!(
        init["result"]["capabilities"]["documentRangeFormattingProvider"],
        serde_json::json!(true),
        "range formatting capability not advertised"
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
    (child, rx, stdin, uri)
}

fn opts(insert_spaces: bool, tab_size: u32) -> serde_json::Value {
    serde_json::json!({"insertSpaces": insert_spaces, "tabSize": tab_size})
}

#[test]
fn formatting_returns_whole_line_edits_and_honours_client_options() {
    let src = "route {\nexit;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("basic", src);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{
            "textDocument":{"uri":uri},"options":opts(true, 2)}}),
    );
    let edits = wait_for(&rx, |v| v["id"] == 2, "formatting");
    let edits = edits["result"].as_array().expect("an edit array");
    assert_eq!(edits.len(), 1, "only the misindented line moves: {edits:?}");
    let e = &edits[0];
    assert_eq!(e["newText"], "  exit;", "client asked for 2 spaces");
    assert_eq!(e["range"]["start"]["line"], 1);
    assert_eq!(
        e["range"]["end"]["line"], 1,
        "an edit never spans a newline"
    );
    assert_eq!(e["range"]["start"]["character"], 0);

    let _ = child.kill();
}

#[test]
fn an_already_formatted_document_produces_no_edits() {
    let (mut child, rx, mut stdin, uri) = boot("clean", "route {\n\texit;\n}\n");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/formatting","params":{
            "textDocument":{"uri":uri},"options":opts(false, 4)}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "formatting");
    assert_eq!(
        r["result"].as_array().map(|a| a.len()),
        Some(0),
        "a clean document was rewritten anyway: {r}"
    );
    let _ = child.kill();
}

#[test]
fn range_formatting_edits_only_the_requested_lines() {
    let src = "route {\nexit;\n}\nroute[A] {\nexit;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("range", src);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/rangeFormatting","params":{
            "textDocument":{"uri":uri},
            "range":{"start":{"line":3,"character":0},"end":{"line":5,"character":1}},
            "options":opts(false, 4)}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "rangeFormatting");
    let edits = r["result"].as_array().expect("an edit array");
    assert!(!edits.is_empty(), "nothing was formatted");
    assert!(
        edits
            .iter()
            .all(|e| (3..=5).contains(&e["range"]["start"]["line"].as_u64().unwrap())),
        "an edit escaped the requested range: {edits:?}"
    );
    let _ = child.kill();
}
