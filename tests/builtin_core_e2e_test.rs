//! The built-in core catalogue, over real LSP stdio, with NOTHING
//! configured.
//!
//! `builtin_core_test.rs` proves the vendored file has the language in
//! it; this proves the running server actually serves it to a client
//! that has set no source tree — the state every user is in on the day
//! they install the extension.  The library-level test passed happily
//! while that path was untested, so the gap is worth a test of its own.

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
    String,
) {
    let dir = std::env::temp_dir().join(format!("oslsp-builtin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    // no tree, no binary: the out-of-the-box state
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_SRC", "")
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
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{}}}),
    );
    // the response has to be read before `initialized` is sent: a
    // notification that arrives mid-handshake is dropped, and the
    // harvest never runs
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let ready = wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "harvest-ready",
    )["params"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    (child, rx, stdin, uri, ready)
}

fn complete(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    line: u32,
    ch: u32,
) -> Vec<String> {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":line,"character":ch}}}),
    );
    let r = wait_for(rx, |v| v["id"] == id, "completion");
    let items = r["result"]
        .as_array()
        .cloned()
        .or_else(|| r["result"]["items"].as_array().cloned())
        .unwrap_or_default();
    items
        .iter()
        .map(|i| i["label"].as_str().unwrap_or("").to_string())
        .collect()
}

/// The complaint that produced the feature: typing `log_` offered
/// nothing but control-flow keywords until a source tree was set.
#[test]
fn a_global_parameter_completes_with_no_source_tree_configured() {
    let (mut child, rx, mut stdin, uri, ready) = boot("params", "log_\n");
    assert!(
        ready.contains("core docs built in from"),
        "the readiness line must say the docs are built in: {ready}"
    );

    let labels = complete(&mut stdin, &rx, &uri, 2, 0, 4);
    assert!(
        labels.contains(&"log_level".to_string()),
        "log_level must complete out of the box; got {} items: {:?}",
        labels.len(),
        &labels[..labels.len().min(25)]
    );
    // and it is not just the keyword list with one entry bolted on
    assert!(
        labels.len() > 100,
        "the whole core language should be offered, not {} items",
        labels.len()
    );
    let _ = child.kill();
}

/// Core functions and pseudo-variables come from the same catalogue,
/// so each needs its own witness rather than one standing for all.
#[test]
fn core_functions_and_pseudo_variables_complete_too() {
    let (mut child, rx, mut stdin, uri, _) = boot("fns", "route {\n    \n}\n");
    let labels = complete(&mut stdin, &rx, &uri, 2, 1, 4);
    assert!(
        labels.iter().any(|l| l == "xlog"),
        "a core function must be offered: {:?}",
        &labels[..labels.len().min(25)]
    );
    let _ = child.kill();

    let (mut child, rx, mut stdin, uri, _) = boot("pvars", "route {\n    $\n}\n");
    let labels = complete(&mut stdin, &rx, &uri, 2, 1, 5);
    assert!(
        labels.iter().any(|l| l.starts_with("$ru") || l == "$ru"),
        "a pseudo-variable must be offered: {:?}",
        &labels[..labels.len().min(25)]
    );
    let _ = child.kill();
}

/// Built-in documentation is pinned to one version, and a user whose
/// build differs has to be able to tell.  Hover says so.
#[test]
fn hover_on_a_built_in_entry_names_the_version_and_the_escape_hatch() {
    let (mut child, rx, mut stdin, uri, _) = boot("hover", "log_level=2\n");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":0,"character":3}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "hover");
    let text = r["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        text.contains("Built-in documentation from OpenSIPS"),
        "hover must disclose the provenance: {text:?}"
    );
    assert!(
        text.contains("opensipsSrc"),
        "hover must name the setting that overrides it: {text:?}"
    );
    let _ = child.kill();
}
