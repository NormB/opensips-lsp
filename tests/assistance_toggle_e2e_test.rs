//! A key sequence that turns hovers and completion off, and on again.
//!
//! GIVEN a reader who is reading rather than writing — walking a
//! colleague's configuration, or demonstrating one — and the popups
//! are in the way,
//! WHEN they press the toggle,
//! THEN hovers and completion stop, immediately, and come back the
//! same way.
//!
//! It must be instant: a setting that restarts the server to take
//! effect is not a toggle, it is a pause. So `assistance` is pushed
//! live over `workspace/didChangeConfiguration` and is deliberately
//! NOT in the client's restart list.
//!
//! Diagnostics are not part of it. They answer "is this
//! configuration valid", which is not noise while reading, and there
//! is already a separate switch for them.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn manifest() -> serde_json::Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("client/package.json");
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn extension_ts() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("client/src/extension.ts");
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn the_manifest_declares_the_command_and_a_key_sequence() {
    let m = manifest();
    let cmds = m["contributes"]["commands"].as_array().expect("commands");
    let toggle = cmds
        .iter()
        .find(|c| c["command"] == "opensipsLsp.toggleAssistance")
        .expect("the toggle command is contributed, or nothing can invoke it");
    assert!(
        toggle["title"].as_str().is_some_and(|t| !t.is_empty()),
        "it needs a title to appear in the command palette: {toggle}"
    );

    let keys = m["contributes"]["keybindings"]
        .as_array()
        .expect("a key sequence, which is what was asked for");
    let bind = keys
        .iter()
        .find(|k| k["command"] == "opensipsLsp.toggleAssistance")
        .expect("the command is bound to a key");
    assert!(
        bind["key"].as_str().is_some_and(|k| k.contains('+')),
        "a chord, not a bare key: {bind}"
    );
    assert!(
        bind["mac"].as_str().is_some_and(|k| k.contains("cmd")),
        "macOS uses cmd where the others use ctrl: {bind}"
    );
}

/// A niche extension must not claim a chord across the whole editor.
#[test]
fn the_key_sequence_only_applies_to_this_language() {
    let m = manifest();
    let bind = m["contributes"]["keybindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["command"] == "opensipsLsp.toggleAssistance")
        .unwrap()
        .clone();
    let when = bind["when"].as_str().unwrap_or("");
    assert!(
        when.contains("editorLangId"),
        "an unscoped binding steals the chord from every other editor \
         window: {bind}"
    );
}

#[test]
fn the_setting_exists_and_defaults_to_on() {
    let m = manifest();
    let p = &m["contributes"]["configuration"]["properties"]["opensipsLsp.assistance"];
    assert_eq!(p["type"], "boolean", "missing or wrong type: {p}");
    assert_eq!(p["default"], true, "assistance is the normal state: {p}");
}

/// The whole point is that it is instant.
#[test]
fn the_toggle_is_not_in_the_restart_list() {
    let ext = extension_ts();
    let start = ext.find("restartSettings").expect("restart list");
    let end = ext[start..].find("];").expect("restart list ends") + start;
    assert!(
        !ext[start..end].contains("assistance"),
        "a toggle that restarts the server is a pause, not a toggle"
    );
    // POSITIVE CONTROL: the list is the real one, not an empty slice
    assert!(
        ext[start..end].contains("opensipsLsp.serverPath"),
        "the restart list read empty — this gate would pass on anything"
    );
}

#[test]
fn the_client_pushes_the_toggle_live() {
    let ext = extension_ts();
    assert!(
        ext.contains("assistance: cfg.get<boolean>('assistance', true)"),
        "the live didChangeConfiguration payload must carry it, or flipping \
         the setting reaches the server only on the next restart"
    );
}

fn boot(
    tag: &str,
    assistance: Option<bool>,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let base = std::env::temp_dir().join(format!("oslsp-assist-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("t.cfg");
    let text = "route {\n    xlog(\"L_INFO\", \"x\");\n}\n";
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut opts = serde_json::json!({ "opensipsPath": "" });
    if let Some(a) = assistance {
        opts["assistance"] = serde_json::json!(a);
    }
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env_remove("OPENSIPS_LSP_BIN")
            .env_remove("OPENSIPS_LSP_SRC")
            .env_remove("OPENSIPS_LSP_ASSISTANCE")
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
            "capabilities":{}, "initializationOptions": opts}}),
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
    (child, rx, stdin, uri)
}

fn hover(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
) -> serde_json::Value {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":5}}}),
    );
    wait_for(rx, |v| v["id"] == id, "hover")["result"].clone()
}

fn completion(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
) -> Vec<String> {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":8}}}),
    );
    let v = wait_for(rx, |v| v["id"] == id, "completion");
    v["result"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|i| i["label"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn assistance_is_on_unless_it_is_turned_off() {
    let (mut child, rx, mut stdin, uri) = boot("on", None);
    assert!(
        !hover(&mut stdin, &rx, &uri, 2).is_null(),
        "the default must be a working editor"
    );
    assert!(
        !completion(&mut stdin, &rx, &uri, 3).is_empty(),
        "and completion must answer"
    );
    let _ = child.kill();
}

#[test]
fn turning_it_off_stops_hovers_and_completion() {
    let (mut child, rx, mut stdin, uri) = boot("off", Some(false));
    assert!(
        hover(&mut stdin, &rx, &uri, 2).is_null(),
        "hover must answer nothing when assistance is off"
    );
    assert!(
        completion(&mut stdin, &rx, &uri, 3).is_empty(),
        "and completion must offer nothing"
    );
    let _ = child.kill();
}

#[test]
fn the_toggle_takes_effect_without_a_restart() {
    let (mut child, rx, mut stdin, uri) = boot("live", None);
    assert!(
        !hover(&mut stdin, &rx, &uri, 2).is_null(),
        "on to begin with"
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
            "settings":{"assistance": false}}}),
    );
    // PROBE, not a sleep: the notification and the request may be
    // handled concurrently, so ask again until the answer changes,
    // bounded — a single ask can beat the notification and read as
    // "the toggle does not work"
    let mut id = 3;
    let off = loop {
        if hover(&mut stdin, &rx, &uri, id).is_null() {
            break true;
        }
        id += 1;
        if id > 12 {
            break false;
        }
    };
    assert!(off, "the same server, no restart, must stop answering");
    assert!(completion(&mut stdin, &rx, &uri, 20).is_empty());

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
            "settings":{"assistance": true}}}),
    );
    assert!(
        !hover(&mut stdin, &rx, &uri, 21).is_null(),
        "and it must come back the same way"
    );
    assert!(!completion(&mut stdin, &rx, &uri, 22).is_empty());
    let _ = child.kill();
}

/// Reading a configuration is not the same as not checking it.
#[test]
fn diagnostics_are_not_part_of_the_toggle() {
    let ext = extension_ts();
    let start = ext.find("toggleAssistance").expect("the command exists");
    let window = &ext[start..(start + 900).min(ext.len())];
    assert!(
        !window.contains("diagnostics"),
        "the toggle silences popups, not the answer to whether the file is \
         valid — which has its own switch"
    );
}

/// Owed for the docs-parity failure this change caused.
///
/// A payload that does not mention `assistance` must leave it alone.
/// The client sends the whole settings block on every change, but an
/// editor that sends only what changed — or a user driving the
/// server directly — would otherwise turn the popups back on by
/// changing something unrelated.
#[test]
fn a_payload_that_omits_the_toggle_leaves_it_alone() {
    let (mut child, rx, mut stdin, uri) = boot("keep", Some(false));
    assert!(
        hover(&mut stdin, &rx, &uri, 2).is_null(),
        "off to begin with"
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
            "settings":{"maxDiagnostics": 42}}}),
    );
    let mut id = 3;
    let stayed_off = loop {
        if !hover(&mut stdin, &rx, &uri, id).is_null() {
            break false;
        }
        id += 1;
        if id > 8 {
            break true;
        }
    };
    assert!(
        stayed_off,
        "changing an unrelated setting turned the popups back on"
    );
    let _ = child.kill();
}

/// Owed, second. The toggle is nested under the namespace when VS
/// Code sends it, and flat when a bare client does; both shapes reach
/// the same switch, and a toggle that works in one editor and not the
/// other is worse than one that works in neither.
#[test]
fn the_namespaced_payload_shape_toggles_too() {
    let (mut child, rx, mut stdin, uri) = boot("nested", None);
    assert!(
        !hover(&mut stdin, &rx, &uri, 2).is_null(),
        "on to begin with"
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
            "settings":{"opensipsLsp":{"assistance": false}}}}),
    );
    let mut id = 3;
    let off = loop {
        if hover(&mut stdin, &rx, &uri, id).is_null() {
            break true;
        }
        id += 1;
        if id > 12 {
            break false;
        }
    };
    assert!(off, "the nested shape must reach the same switch");
    let _ = child.kill();
}

/// Every toggle the client pushes live must also be sent at startup.
///
/// `assistance` was pushed live and not sent in
/// `initializationOptions`: turn the popups off, close the editor,
/// reopen it, and they are back — the setting says off and the server
/// has never been told. Every other runtime toggle was in both, so
/// nothing distinguished the one that was in only one of them.
#[test]
fn every_live_toggle_is_also_sent_at_startup() {
    let ext = extension_ts();
    let init_start = ext.find("initializationOptions: {").expect("init options");
    let init_end = ext[init_start..].find("\n        },").expect("init ends") + init_start;
    let init = &ext[init_start..init_end];

    let live_start = ext
        .find("sendNotification('workspace/didChangeConfiguration'")
        .expect("live push");
    let live_end = ext[live_start..]
        .find("\n            });")
        .expect("live ends")
        + live_start;
    let live = &ext[live_start..live_end];

    let keys = |block: &str| -> Vec<String> {
        block
            .lines()
            .filter_map(|l| l.trim().split_once(": cfg.get").map(|(k, _)| k.to_string()))
            .collect()
    };
    let (init_keys, live_keys) = (keys(init), keys(live));

    // POSITIVE CONTROL: both blocks parsed, so a missing key below is
    // a real absence and not an empty scan
    assert!(init_keys.len() > 4, "init options read as {init_keys:?}");
    assert!(live_keys.len() > 4, "live payload read as {live_keys:?}");
    assert!(live_keys.iter().any(|k| k == "assistance"), "{live_keys:?}");

    for k in &live_keys {
        assert!(
            init_keys.contains(k),
            "`{k}` is pushed live but not sent at startup, so it is forgotten \
             every time the editor restarts: init {init_keys:?}"
        );
    }
}
