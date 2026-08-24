//! Refactorings over real LSP stdio, and the proof that the edits are
//! applicable: each one is applied to the buffer here and the result
//! checked, because a `WorkspaceEdit` that looks right in JSON and
//! corrupts the file is the failure that matters.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn boot(
    tag: &str,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let dir = std::env::temp_dir().join(format!("oslsp-rf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
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
    wait_for(&rx, |v| v["id"] == 1, "initialize");
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

fn actions(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    from: u32,
    to: u32,
) -> Vec<serde_json::Value> {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/codeAction","params":{
            "textDocument":{"uri":uri},
            "range":{"start":{"line":from,"character":0},"end":{"line":to,"character":0}},
            "context":{"diagnostics":[]}}}),
    );
    wait_for(rx, |v| v["id"] == id, "codeAction")["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Apply a `WorkspaceEdit`'s line-oriented edits to `text`, the way a
/// client does: last edit first, so earlier ranges stay valid.
fn apply(text: &str, action: &serde_json::Value, uri: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut edits: Vec<&serde_json::Value> = action["edit"]["changes"][uri]
        .as_array()
        .expect("edits")
        .iter()
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e["range"]["start"]["line"].as_u64().unwrap()));
    for e in edits {
        let s = e["range"]["start"]["line"].as_u64().unwrap() as usize;
        let el = e["range"]["end"]["line"].as_u64().unwrap() as usize;
        let ec = e["range"]["end"]["character"].as_u64().unwrap();
        let new = e["newText"].as_str().unwrap();
        if ec == 0 && el > s {
            // a whole-line range: replace those lines outright
            lines.splice(s..el, new.lines().map(str::to_string));
        } else if ec == 0 && el == s {
            // an insertion at the start of a line
            lines.splice(s..s, new.lines().map(str::to_string));
        } else {
            lines.splice(s..=el, new.lines().map(str::to_string));
        }
    }
    lines.join("\n") + "\n"
}

#[test]
fn extracting_a_selection_produces_an_applicable_edit() {
    let src =
        "loadmodule \"tm.so\"\nroute {\n    xlog(\"one\");\n    xlog(\"two\");\n    exit;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("extract", src);
    let acts = actions(&mut stdin, &rx, &uri, 2, 2, 4);
    let extract = acts
        .iter()
        .find(|a| a["kind"] == "refactor.extract")
        .expect("an extract action");
    assert_eq!(extract["title"], "Extract into route[EXTRACTED]");

    let out = apply(src, extract, &uri);
    assert_eq!(
        out,
        "loadmodule \"tm.so\"\nroute {\n    route(EXTRACTED);\n    exit;\n}\n\
         route[EXTRACTED] {\n    xlog(\"one\");\n    xlog(\"two\");\n}\n",
        "applied result:\n{out}"
    );
    let _ = child.kill();
}

#[test]
fn a_selection_that_would_change_behaviour_offers_no_extraction() {
    let src = "route {\n    xlog(\"x\");\n    return;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("return", src);
    let acts = actions(&mut stdin, &rx, &uri, 2, 1, 3);
    assert!(
        !acts.iter().any(|a| a["kind"] == "refactor.extract"),
        "a selection containing `return` must not be extractable: {acts:?}"
    );
    let _ = child.kill();
}

#[test]
fn duplicate_loadmodules_can_be_removed_and_the_result_is_clean() {
    let src =
        "loadmodule \"tm.so\"\nloadmodule \"sl.so\"\nloadmodule \"tm.so\"\nroute {\n    exit;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("dupes", src);
    let acts = actions(&mut stdin, &rx, &uri, 2, 0, 0);
    let organize = acts
        .iter()
        .find(|a| a["kind"] == "source.organizeImports")
        .expect("an organize action");
    assert_eq!(organize["title"], "Remove 1 duplicate loadmodule line");

    let out = apply(src, organize, &uri);
    assert_eq!(
        out, "loadmodule \"tm.so\"\nloadmodule \"sl.so\"\nroute {\n    exit;\n}\n",
        "applied result:\n{out}"
    );
    // the surviving order is the original order: nothing was sorted
    assert!(
        out.find("tm.so").unwrap() < out.find("sl.so").unwrap(),
        "load order must not change"
    );
    let _ = child.kill();
}

#[test]
fn a_document_without_duplicates_offers_no_removal() {
    let src = "loadmodule \"tm.so\"\nroute {\n    exit;\n}\n";
    let (mut child, rx, mut stdin, uri) = boot("nodupes", src);
    let acts = actions(&mut stdin, &rx, &uri, 2, 0, 0);
    assert!(
        !acts.iter().any(|a| a["kind"] == "source.organizeImports"),
        "{acts:?}"
    );
    let _ = child.kill();
}
