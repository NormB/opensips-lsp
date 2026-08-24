//! Call hierarchy over real LSP stdio.
//!
//! The graph is the route table: `route(NAME)` is the only call form
//! the server can see, so the main table is what takes part. A
//! `failure_route[NAME]` is armed by a module function string the
//! server does not track — it can *make* calls, but it has no callers
//! to report, and claiming otherwise would be worse than declining.

mod common;
use common::*;
use std::process::{Command, Stdio};

const MAIN: &str = "\
route {
    route(RELAY);
    route(RELAY);
}
route[RELAY] {
    route(DEEPER);
    route(GONE);
}
route[DEEPER] {
    exit;
}
failure_route[RELAY] {
    route(RELAY);
}
";

fn boot(
    tag: &str,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let dir = std::env::temp_dir().join(format!("oslsp-ch-{tag}-{}", std::process::id()));
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
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize");
    assert_eq!(
        init["result"]["capabilities"]["callHierarchyProvider"],
        serde_json::json!(true),
        "call hierarchy capability not advertised"
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

fn prepare(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    line: u32,
    ch: u32,
) -> serde_json::Value {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/prepareCallHierarchy",
            "params":{"textDocument":{"uri":uri},"position":{"line":line,"character":ch}}}),
    );
    wait_for(rx, |v| v["id"] == id, "prepareCallHierarchy")["result"].clone()
}

#[test]
fn prepare_resolves_a_call_site_to_the_route_it_targets() {
    let (mut child, rx, mut stdin, uri) = boot("prep", MAIN);
    // cursor on RELAY inside `route(RELAY);` on line 1
    let r = prepare(&mut stdin, &rx, &uri, 2, 1, 11);
    let items = r.as_array().expect("an item array");
    assert_eq!(items.len(), 1, "expected one item: {r}");
    assert_eq!(items[0]["name"], "route[RELAY]");
    // the item anchors on the DEFINITION, not the call site
    assert_eq!(items[0]["selectionRange"]["start"]["line"], 4);
    assert_eq!(items[0]["data"]["route"], "RELAY");
    let _ = child.kill();
}

#[test]
fn a_per_kind_block_has_no_hierarchy_of_its_own() {
    let (mut child, rx, mut stdin, uri) = boot("kind", MAIN);
    // cursor on RELAY in `failure_route[RELAY]` (line 11)
    let r = prepare(&mut stdin, &rx, &uri, 2, 11, 15);
    assert!(
        r.is_null() || r.as_array().map(|a| a.is_empty()).unwrap_or(false),
        "a failure_route must not claim main-table callers: {r}"
    );
    let _ = child.kill();
}

#[test]
fn incoming_calls_group_by_caller_and_keep_every_call_site() {
    let (mut child, rx, mut stdin, uri) = boot("in", MAIN);
    let item = prepare(&mut stdin, &rx, &uri, 2, 1, 11)[0].clone();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"callHierarchy/incomingCalls",
            "params":{"item":item}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "incomingCalls");
    let calls = r["result"].as_array().expect("a call array");

    let names: Vec<&str> = calls
        .iter()
        .map(|c| c["from"]["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"route"), "the main route calls RELAY twice");
    assert!(
        names.contains(&"failure_route[RELAY]"),
        "a failure_route calling route(RELAY) is a real incoming edge: {names:?}"
    );

    let main = calls
        .iter()
        .find(|c| c["from"]["name"] == "route")
        .expect("the main block");
    assert_eq!(
        main["fromRanges"].as_array().unwrap().len(),
        2,
        "two call sites in one caller collapse to one entry with two ranges"
    );
    let _ = child.kill();
}

#[test]
fn outgoing_calls_list_targets_and_name_the_undefined_ones() {
    let (mut child, rx, mut stdin, uri) = boot("out", MAIN);
    let item = prepare(&mut stdin, &rx, &uri, 2, 1, 11)[0].clone();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"callHierarchy/outgoingCalls",
            "params":{"item":item}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "outgoingCalls");
    let calls = r["result"].as_array().expect("a call array");
    let names: Vec<&str> = calls
        .iter()
        .map(|c| c["to"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["route[DEEPER]", "route[GONE]"]);

    // an edge to a route defined nowhere is still an edge: dropping it
    // would hide a call the reader can see in the file
    let gone = calls
        .iter()
        .find(|c| c["to"]["name"] == "route[GONE]")
        .unwrap();
    assert_eq!(gone["to"]["detail"], "undefined");
    let _ = child.kill();
}

#[test]
fn the_graph_spans_the_include_closure() {
    let dir = std::env::temp_dir().join(format!("oslsp-ch-inc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("inc.cfg"),
        "route[FROM_INCLUDE] {\n    route(TARGET);\n}\n",
    )
    .unwrap();
    let main = "include_file \"inc.cfg\"\nroute[TARGET] {\n    exit;\n}\n";
    let cfg = dir.join("main.cfg");
    std::fs::write(&cfg, main).unwrap();
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
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":main}}}),
    );

    // cursor on TARGET in `route[TARGET]` (line 1)
    let item = prepare(&mut stdin, &rx, &uri, 2, 1, 6)[0].clone();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"callHierarchy/incomingCalls",
            "params":{"item":item}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "incomingCalls");
    let calls = r["result"].as_array().expect("a call array");
    assert_eq!(
        calls.len(),
        1,
        "the caller lives in an included file: {}",
        r["result"]
    );
    assert_eq!(calls[0]["from"]["name"], "route[FROM_INCLUDE]");
    assert!(
        calls[0]["from"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("inc.cfg"),
        "the caller item must point at the included file"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
}
