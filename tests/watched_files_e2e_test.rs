//! `workspace/didChangeWatchedFiles`: keeping up with changes that
//! never arrive as a document edit.
//!
//! Two of them matter. A config included by an open file can change
//! under a git checkout or another tool, and the documentation tree
//! can be rebuilt or switched. Without a watcher the server keeps
//! answering from a stale read until the buffer happens to be touched.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn boot(
    tag: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    std::path::PathBuf,
    serde_json::Value,
) {
    let base = std::env::temp_dir().join(format!("oslsp-watch-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("tree/modules/mymod")).unwrap();
    std::fs::write(
        base.join("tree/modules/mymod/README.md"),
        "# mymod\n\n## Admin Guide\n\n### Exported Functions\n\n#### first_fn()\n\nOne.\n",
    )
    .unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_SRC", base.join("tree").display().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &base,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true}}}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // answer the registration the way a client would, then wait for
    // the harvest so the tests exercise a ready server
    let req = wait_for(
        &rx,
        |v| v["method"] == "client/registerCapability",
        "registerCapability",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":req["id"].clone(),"result":null}),
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
        "harvest-ready",
    );
    (child, rx, stdin, base, req)
}

/// The server must ask to be told: a watcher nobody registered is a
/// watcher that never fires.
#[test]
fn the_server_registers_for_the_files_it_reads() {
    let (mut child, _rx, _stdin, base, req) = boot("register");

    let regs = req["params"]["registrations"]
        .as_array()
        .expect("registrations");
    let watched = regs
        .iter()
        .find(|r| r["method"] == "workspace/didChangeWatchedFiles")
        .expect("a didChangeWatchedFiles registration");
    let watchers = watched["registerOptions"]["watchers"]
        .as_array()
        .expect("watchers");
    let patterns: Vec<String> = watchers
        .iter()
        .map(|w| w["globPattern"].to_string())
        .collect();
    assert!(
        patterns.iter().any(|p| p.contains("*.cfg")),
        "configs must be watched: {patterns:?}"
    );
    assert!(
        patterns.iter().any(|p| p.contains("README.md")),
        "the documentation tree must be watched: {patterns:?}"
    );
    assert!(
        patterns.iter().any(|p| p.contains("baseUri")),
        "the tree lives outside the workspace, so its watcher is relative to the tree: {patterns:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// An include edited on disk changes what the open document means,
/// and the change never reaches the server as an edit.
///
/// Both directions are checked.  A clean document publishes nothing at
/// all here (the checker is off), so proving the warning appears AND
/// then clears is what shows the republish is driven by the include
/// rather than by chance.
#[test]
fn an_include_changing_on_disk_republishes_diagnostics() {
    let (mut child, rx, mut stdin, base, _) = boot("include");
    let inc = base.join("inc.cfg");
    let notify = |stdin: &mut std::process::ChildStdin| {
        write_msg(
            stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
                "changes":[{"uri": format!("file://{}", inc.display()), "type": 2}]}}),
        );
    };

    std::fs::write(&inc, "route[HELPER] {\n    exit;\n}\n").unwrap();
    let main = "include_file \"inc.cfg\"\nroute {\n    route(HELPER);\n}\n";
    let cfg = base.join("main.cfg");
    std::fs::write(&cfg, main).unwrap();
    let uri = format!("file://{}", cfg.display());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":main}}}),
    );

    // the include loses the route, without the buffer being touched
    std::fs::write(&inc, "route[SOMETHING_ELSE] {\n    exit;\n}\n").unwrap();
    notify(&mut stdin);
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true)
        },
        "warning after the include lost the route",
    );
    let msg = d["params"]["diagnostics"][0]["message"]
        .as_str()
        .unwrap_or("");
    assert!(
        msg.contains("HELPER"),
        "expected an undefined-route warning: {msg}"
    );

    // and putting it back clears the warning, again with no edit
    std::fs::write(&inc, "route[HELPER] {\n    exit;\n}\n").unwrap();
    notify(&mut stdin);
    let d = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["diagnostics"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(false)
        },
        "warning cleared after the include regained the route",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().map(|a| a.len()),
        Some(0)
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A rebuilt documentation tree must be re-read, not served from the
/// cache entry made before it changed.
#[test]
fn a_changed_documentation_tree_is_reharvested() {
    let (mut child, rx, mut stdin, base, _) = boot("tree");
    let cfg = base.join("t.cfg");
    let text = "loadmodule \"mymod.so\"\nroute {\n    \n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );

    let ask = |stdin: &mut std::process::ChildStdin, id: i64| {
        write_msg(
            stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{
                "textDocument":{"uri":uri},"position":{"line":2,"character":4}}}),
        );
    };
    ask(&mut stdin, 10);
    let r = wait_for(&rx, |v| v["id"] == 10, "completion before");
    let labels: Vec<String> = r["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|i| i["label"].as_str().unwrap_or("").to_string())
        .collect();
    assert!(labels.contains(&"first_fn".to_string()), "{labels:?}");
    assert!(!labels.contains(&"second_fn".to_string()));

    // the module gains a function on disk
    std::fs::write(
        base.join("tree/modules/mymod/README.md"),
        "# mymod\n\n## Admin Guide\n\n### Exported Functions\n\n\
         #### first_fn()\n\nOne.\n\n#### second_fn()\n\nTwo.\n",
    )
    .unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
            "changes":[{"uri": format!("file://{}", base.join("tree/modules/mymod/README.md").display()), "type": 2}]}}),
    );

    // the harvest is asynchronous; poll until it lands
    let mut found = false;
    for id in 11..25 {
        ask(&mut stdin, id);
        let r = wait_for(&rx, |v| v["id"] == id, "completion after");
        let labels: Vec<String> = r["result"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|i| i["label"].as_str().unwrap_or("").to_string())
            .collect();
        if labels.contains(&"second_fn".to_string()) {
            found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(found, "the rebuilt tree was never re-read");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}
