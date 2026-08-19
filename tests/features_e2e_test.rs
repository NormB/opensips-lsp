//! Feature-exercise e2e: every advertised capability driven over real
//! stdio LSP against a SYNTHETIC OpenSIPS tree — hermetic, no gating.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn mk_tree(root: &std::path::Path) {
    let w = |p: std::path::PathBuf, c: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    };
    w(
        root.join("modules/mymod/README.md"),
        "# mymod\n\n### Exported Parameters\n\n#### my_param (string)\n\nA parameter.\n\n### Exported Functions\n\n#### my_func(arg)\n\nDoes things.\n",
    );
    w(
        root.join("modules/othermod/README.md"),
        "# othermod\n\n### Exported Functions\n\n#### other_func()\n\nOther things.\n",
    );
    w(
        root.join("docs/manual/Script-CoreFunctions.md"),
        "# Core\n\n## core_fn(x)\n\nA core function.\n",
    );
    w(
        root.join("docs/manual/Script-CoreParameters.md"),
        "# Core\n\n## Core parameters\n\n### core_param\n\nA core parameter.\n",
    );
    w(
        root.join("docs/manual/Script-CoreVar.md"),
        "# Vars\n\n## Reference Variables\n\n### Request URI - $ru\n\nThe request URI.\n",
    );
}

const DOC: &str = "loadmodule \"mymod.so\"\nroute[helper] {\n    my_func(\"x\");\n}\nroute {\n    route(helper);\n}\n";

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
fn all_features_over_stdio_against_a_synthetic_tree() {
    let base = std::env::temp_dir().join(format!("oslsp-feat-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let tree = base.join("tree");
    mk_tree(&tree);
    let cache = base.join("cache");
    let cfg = base.join("t.cfg");
    std::fs::write(&cfg, DOC).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        // config via initializationOptions below, NOT env: proves the
        // option path; env supplies only the cache dir (env-only knob)
        .env_remove("OPENSIPS_LSP_BIN")
        .env_remove("OPENSIPS_LSP_SRC")
        .env("OPENSIPS_LSP_CACHE_DIR", cache.display().to_string())
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
        "capabilities":{},
        "initializationOptions":{
            "opensipsPath":"",
            "opensipsSrc": tree.display().to_string(),
            "checkTimeoutMs": 1234
        }}}),
    );
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
        "readiness",
    );
    let msg = ready["params"]["message"].as_str().unwrap().to_string();
    assert!(msg.contains("2 documented modules"), "harvest count: {msg}");
    assert!(msg.contains("1 core functions"), "core count: {msg}");

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":DOC}}}),
    );

    // completion in code offers loaded-module + core functions + core params
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":2,"character":6}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 2, "code completion"));
    assert!(l.contains(&"my_func".to_string()), "{l:?}");
    assert!(l.contains(&"core_fn".to_string()));
    assert!(l.contains(&"core_param".to_string()));
    assert!(
        !l.contains(&"other_func".to_string()),
        "othermod is NOT loaded: {l:?}"
    );

    // hover over a module function
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":2,"character":5}}}),
    );
    let h = wait_for(&rx, |v| v["id"] == 4, "hover");
    assert!(
        h["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("my_func(arg)"),
        "hover: {h}"
    );

    // goto definition: route(helper) on line 5, cursor on 'helper' (col 11)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":5,"method":"textDocument/definition","params":{
            "textDocument":{"uri":uri},"position":{"line":5,"character":11}}}),
    );
    let d = wait_for(&rx, |v| v["id"] == 5, "definition");
    assert_eq!(d["result"]["range"]["start"]["line"], 1, "definition: {d}");

    // didChange: load othermod too and reference $ru; completion follows the LIVE buffer
    let doc2 = format!("loadmodule \"othermod.so\"\n{DOC}route {{ xlog(\"$\"); }}\n");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},
            "contentChanges":[{"text": doc2}]}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":6,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":3,"character":6}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 6, "post-change completion"));
    assert!(
        l.contains(&"other_func".to_string()),
        "didChange must update the live buffer: {l:?}"
    );

    // pvar completion right after the '$' in the appended route (line 8)
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":7,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":8,"character":15}}}),
    );
    let l = labels(&wait_for(&rx, |v| v["id"] == 7, "pvar completion"));
    assert_eq!(l, vec!["$ru".to_string()], "pvar completion: {l:?}");

    // didClose: the document store forgets the file → hover yields null
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri":uri}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":8,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":2,"character":5}}}),
    );
    let h = wait_for(&rx, |v| v["id"] == 8, "post-close hover");
    assert!(h["result"].is_null(), "closed docs must be forgotten: {h}");

    // second server against the same tree must hit the cache
    child.kill().ok();
    let mut child2 = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env_remove("OPENSIPS_LSP_BIN")
        .env_remove("OPENSIPS_LSP_SRC")
        .env("OPENSIPS_LSP_CACHE_DIR", cache.display().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let rx2 = spawn_reader(&mut child2);
    let mut stdin2 = child2.stdin.take().unwrap();
    write_msg(
        &mut stdin2,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{},
            "initializationOptions":{"opensipsPath":"","opensipsSrc": tree.display().to_string()}}}),
    );
    wait_for(&rx2, |v| v["id"] == 1, "init 2");
    write_msg(
        &mut stdin2,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let ready2 = wait_for(
        &rx2,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "readiness 2",
    );
    assert!(
        ready2["params"]["message"]
            .as_str()
            .unwrap()
            .contains(", cached"),
        "second start must hit the cache: {ready2}"
    );
    child2.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
