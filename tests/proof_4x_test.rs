//! Proof suite: the server, spoken to over real LSP stdio, handles
//! the OpenSIPS 4.x master tree — module docs, core functions, core
//! parameters, pseudo-variables, and (with a real binary) `-C`
//! diagnostics.
//!
//! Gated: set OPENSIPS_LSP_TEST_TREE to a 4.x source tree; set
//! OPENSIPS_LSP_TEST_BIN to a matching opensips binary to also prove
//! the diagnostics leg.

mod common;
use common::*;
use std::process::{Command, Stdio};

const CFG: &str = "log_level=2\nloadmodule \"proto_udp.so\"\nloadmodule \"tm.so\"\nmodparam(\"tm\", \"fr_timeout\", 3)\nroute { exit; }\n";

fn labels(v: &serde_json::Value) -> Vec<String> {
    v["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|i| i["label"].as_str().unwrap_or("").to_string())
        .collect()
}

#[test]
fn full_stack_against_a_real_4x_tree() {
    let Ok(tree) = std::env::var("OPENSIPS_LSP_TEST_TREE") else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_TREE not set");
        return;
    };
    let bin = std::env::var("OPENSIPS_LSP_TEST_BIN").unwrap_or_default();

    let dir = std::env::temp_dir().join(format!("oslsp-proof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("proof.cfg");
    // an unknown modparam so the diagnostics leg has something to find
    let bad = format!("{CFG}modparam(\"tm\", \"no_such_param_xyz\", 1)\n");
    std::fs::write(&cfg, &bad).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", &bin) // empty = diagnostics off
        .env("OPENSIPS_LSP_SRC", &tree)
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
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // the harvest is asynchronous: wait for the readiness log line
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "harvest-ready logMessage",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":bad}}}),
    );

    // diagnostics leg first: wait_for discards non-matching traffic,
    // so this must be consumed before the request/response waits below
    if !bin.is_empty() {
        let d = wait_for(
            &rx,
            |v| {
                v["method"] == "textDocument/publishDiagnostics"
                    && !v["params"]["diagnostics"]
                        .as_array()
                        .unwrap_or(&vec![])
                        .is_empty()
            },
            "real -C diagnostics",
        );
        let msgs: Vec<String> = d["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["message"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(
            msgs.iter().any(|m| m.contains("no_such_param_xyz")),
            "diagnostics: {msgs:?}"
        );
    }

    // 1. module-param completion inside modparam("tm", "...
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":3,"character":16}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 2, "tm param completion"));
    assert!(l.contains(&"fr_timeout".to_string()), "tm params: {l:?}");

    // 2. code position offers module functions AND core functions
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":4,"character":8}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 3, "code completion"));
    assert!(l.contains(&"t_relay".to_string()), "module fn: {l:?}");
    assert!(l.contains(&"cache_store".to_string()), "core fn missing");
    assert!(
        l.contains(&"advertised_address".to_string()),
        "core param missing"
    );

    // 3. hover on a core function documents it
    // (line 4 col: "route { exit; }" — hover the tm modparam name instead)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":3,"character":18}}}),
    );
    let h = wait_for(&rx, |v| v["id"] == 4, "hover");
    let hover = h["result"]["contents"]["value"].as_str().unwrap_or("");
    assert!(hover.contains("fr_timeout"), "hover: {hover}");

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
