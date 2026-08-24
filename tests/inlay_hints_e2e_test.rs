//! Inlay hints over real LSP stdio.
//!
//! Hints come from the documentation catalogue, which is what keeps
//! keywords out without special-casing them: `if (...)` is not a
//! documented function, so it gets nothing.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn mk_tree(root: &std::path::Path) {
    let w = |p: std::path::PathBuf, c: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    };
    w(
        root.join("modules/tm/README.md"),
        "# tm\n\n## Admin Guide\n\n### Exported Functions\n\n\
         #### t_relay([flags],[outbound_proxy])\n\nRelay it.\n\n\
         #### t_newtran()\n\nNew transaction.\n",
    );
}

const CFG: &str = "\
loadmodule \"tm.so\"
route {
    t_relay(\"udp\", 1);
    t_newtran();
    if (1) {
        exit;
    }
}
";

fn boot(
    tag: &str,
    opts: serde_json::Value,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
) {
    let base = std::env::temp_dir().join(format!("oslsp-hint-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    mk_tree(&tree);
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, CFG).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_SRC", tree.display().to_string())
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
    let init = wait_for(&rx, |v| v["id"] == 1, "initialize");
    assert_eq!(
        init["result"]["capabilities"]["inlayHintProvider"],
        serde_json::json!(true),
        "inlay hint capability not advertised"
    );
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
        "harvest-ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":CFG}}}),
    );
    (child, rx, stdin, uri)
}

fn hints(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    from: u32,
    to: u32,
) -> Vec<(u64, u64, String)> {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/inlayHint","params":{
            "textDocument":{"uri":uri},
            "range":{"start":{"line":from,"character":0},"end":{"line":to,"character":0}}}}),
    );
    let r = wait_for(rx, |v| v["id"] == id, "inlayHint");
    r["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|h| {
            (
                h["position"]["line"].as_u64().unwrap_or(0),
                h["position"]["character"].as_u64().unwrap_or(0),
                h["label"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn documented_call_arguments_get_their_parameter_names() {
    let (mut child, rx, mut stdin, uri) = boot("basic", serde_json::json!({}));
    let got = hints(&mut stdin, &rx, &uri, 2, 0, 20);
    assert_eq!(
        got,
        vec![
            (2, 12, "flags:".to_string()),
            (2, 19, "outbound_proxy:".to_string()),
        ],
        "bracket markers must be stripped and both arguments hinted"
    );
    let _ = child.kill();
}

#[test]
fn undocumented_calls_and_keywords_get_nothing() {
    let (mut child, rx, mut stdin, uri) = boot("keywords", serde_json::json!({}));
    let got = hints(&mut stdin, &rx, &uri, 2, 0, 20);
    assert!(
        got.iter().all(|(l, _, _)| *l == 2),
        "only the documented call on line 2 is hinted: {got:?}"
    );
    let _ = child.kill();
}

#[test]
fn a_call_with_no_arguments_is_not_hinted() {
    let (mut child, rx, mut stdin, uri) = boot("noargs", serde_json::json!({}));
    let got = hints(&mut stdin, &rx, &uri, 2, 3, 3);
    assert!(got.is_empty(), "t_newtran() takes nothing: {got:?}");
    let _ = child.kill();
}

#[test]
fn hints_are_limited_to_the_requested_range() {
    let (mut child, rx, mut stdin, uri) = boot("range", serde_json::json!({}));
    let got = hints(&mut stdin, &rx, &uri, 2, 4, 8);
    assert!(got.is_empty(), "line 2 is outside the viewport: {got:?}");
    let _ = child.kill();
}

#[test]
fn the_setting_turns_them_off() {
    let (mut child, rx, mut stdin, uri) =
        boot("off", serde_json::json!({"inlayHintParameterNames": false}));
    let got = hints(&mut stdin, &rx, &uri, 2, 0, 20);
    assert!(got.is_empty(), "hints were disabled: {got:?}");
    let _ = child.kill();
}

#[test]
fn the_setting_applies_live() {
    let (mut child, rx, mut stdin, uri) = boot(
        "live",
        serde_json::json!({"inlayHintParameterNames": false}),
    );
    assert!(hints(&mut stdin, &rx, &uri, 2, 0, 20).is_empty());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration","params":{
            "settings":{"opensipsLsp":{"inlayHintParameterNames": true}}}}),
    );
    assert!(
        !hints(&mut stdin, &rx, &uri, 3, 0, 20).is_empty(),
        "the toggle must apply without a restart"
    );
    let _ = child.kill();
}
