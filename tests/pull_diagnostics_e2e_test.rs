//! Pull diagnostics: `textDocument/diagnostic` and
//! `workspace/diagnostic`.
//!
//! The workspace half carries the interesting decision. A config that
//! another config includes is a fragment, not a program: checking it
//! on its own would flag every route its parent defines as undefined.
//! Only roots — the files nothing else includes — are reported, and
//! their closures already cover the fragments.

mod common;
use common::*;
use std::process::{Command, Stdio};

/// A client that pulls, as VS Code does.
fn caps(pull: bool) -> serde_json::Value {
    if pull {
        serde_json::json!({
            "textDocument": {"diagnostic": {"dynamicRegistration": false}},
            "workspace": {"diagnostics": {"refreshSupport": true}}
        })
    } else {
        serde_json::json!({})
    }
}

fn boot(
    tag: &str,
    pull: bool,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("oslsp-pull-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
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
            "capabilities": caps(pull),
            "workspaceFolders":[{"uri": format!("file://{}", base.display()), "name":"w"}]}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize");
    let provider = &init["result"]["capabilities"]["diagnosticProvider"];
    assert_eq!(provider["interFileDependencies"], serde_json::json!(true));
    assert_eq!(provider["workspaceDiagnostics"], serde_json::json!(true));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin, base)
}

fn open(stdin: &mut std::process::ChildStdin, uri: &str, text: &str) {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
}

#[test]
fn a_document_report_carries_the_analyzer_findings() {
    let (mut child, rx, mut stdin, base) = boot("doc", true);
    let cfg = base.join("t.cfg");
    let text = "route {\n    route(MISSING);\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    open(&mut stdin, &uri, text);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/diagnostic","params":{
            "textDocument":{"uri":uri}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 2, "diagnostic");
    assert_eq!(r["result"]["kind"], "full");
    let items = r["result"]["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{}", r["result"]);
    assert!(
        items[0]["message"]
            .as_str()
            .unwrap_or("")
            .contains("MISSING")
    );
    assert!(
        r["result"]["resultId"].as_str().is_some(),
        "a report needs an identity for the unchanged case"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn an_unchanged_document_reports_unchanged() {
    let (mut child, rx, mut stdin, base) = boot("unchanged", true);
    let cfg = base.join("t.cfg");
    let text = "route {\n    route(MISSING);\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    open(&mut stdin, &uri, text);

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/diagnostic","params":{
            "textDocument":{"uri":uri}}}),
    );
    let first = wait_for(&rx, |v| v["id"] == 2, "diagnostic");
    let id = first["result"]["resultId"].as_str().unwrap().to_string();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/diagnostic","params":{
            "textDocument":{"uri":uri},"previousResultId": id}}),
    );
    let again = wait_for(&rx, |v| v["id"] == 3, "diagnostic again");
    assert_eq!(
        again["result"]["kind"], "unchanged",
        "nothing moved, so the report must not resend: {}",
        again["result"]
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_pulling_client_is_not_also_pushed_to() {
    let (mut child, rx, mut stdin, base) = boot("nopush", true);
    let cfg = base.join("t.cfg");
    let text = "route {\n    route(MISSING);\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    open(&mut stdin, &uri, text);
    // an edit would normally trigger the debounced analyzer publish
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":"route {\n    route(STILL_MISSING);\n}\n"}]}}),
    );
    // give the debounce time to fire, then prove nothing was pushed by
    // asking for something else and seeing that answer arrive first
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"textDocument/diagnostic","params":{
            "textDocument":{"uri":uri}}}),
    );
    let mut pushed = false;
    loop {
        let v = wait_for(
            &rx,
            |v| v["id"] == 9 || v["method"] == "textDocument/publishDiagnostics",
            "pull answer or a push",
        );
        if v["method"] == "textDocument/publishDiagnostics" {
            pushed = true;
            continue;
        }
        break;
    }
    assert!(
        !pushed,
        "a pulling client must not also receive pushed diagnostics"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_pushing_client_still_gets_pushed_diagnostics() {
    let (mut child, rx, mut stdin, base) = boot("push", false);
    let cfg = base.join("t.cfg");
    let text = "route {\n    route(MISSING);\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());
    open(&mut stdin, &uri, text);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text":"route {\n    route(STILL_MISSING);\n}\n"}]}}),
    );
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "push for a non-pulling client",
    );
    assert!(
        !d["params"]["diagnostics"]
            .as_array()
            .unwrap_or(&vec![])
            .is_empty()
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// The decision that makes workspace diagnostics honest.
#[test]
fn only_root_configs_are_reported_never_the_fragments_they_include() {
    let (mut child, rx, mut stdin, base) = boot("workspace", true);
    // main.cfg defines HELPER and includes the fragment that calls it;
    // the fragment on its own would look broken
    std::fs::write(
        base.join("frag.cfg"),
        "route[CALLER] {\n    route(HELPER);\n}\n",
    )
    .unwrap();
    std::fs::write(
        base.join("main.cfg"),
        "include_file \"frag.cfg\"\nroute[HELPER] {\n    exit;\n}\nroute {\n    route(GHOST);\n}\n",
    )
    .unwrap();

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"workspace/diagnostic","params":{
            "previousResultIds":[]}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 5, "workspace/diagnostic");
    let items = r["result"]["items"].as_array().expect("items");
    let reported: Vec<&str> = items
        .iter()
        .map(|i| i["uri"].as_str().unwrap_or(""))
        .collect();

    assert!(
        reported.iter().any(|u| u.ends_with("main.cfg")),
        "the root must be reported: {reported:?}"
    );
    assert!(
        !reported.iter().any(|u| u.ends_with("frag.cfg")),
        "an included fragment is not a program and must not be reported: {reported:?}"
    );
    // and the root's own real problem is there
    let main = items
        .iter()
        .find(|i| i["uri"].as_str().unwrap_or("").ends_with("main.cfg"))
        .unwrap();
    let msgs: Vec<&str> = main["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["message"].as_str().unwrap_or(""))
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("GHOST")),
        "the root's undefined route should show: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("HELPER")),
        "HELPER is defined in the root, so the fragment's call is fine: {msgs:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}
