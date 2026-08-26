//! `opensipsVersion` must reach the editor's diagnostics, not just
//! the CLI's.
//!
//! The setting is read in `initialize` and the chosen catalogue is
//! threaded through every publish path by hand. That threading is the
//! part that can silently come undone: make the server ignore the
//! setting and the CLI tests still pass, the unit tests on
//! `catalog_diagnostics` still pass, and an editor quietly checks
//! against a release the user did not ask for.
//!
//! So this asserts the CONSEQUENCE rather than the header.
//! `proto_bin::bin_async_local_connect_timeout` exists in 3.5.9 and
//! 3.6.8 and was dropped by 4.0: on the newest release it must warn
//! and say where the name still exists, and on 3.6.8 it must be
//! silent, because that release really does export it.

mod common;
use common::*;
use std::process::{Command, Stdio};

const CFG: &str = "route {\n    xlog(\"x\");\n}\nmodparam(\"proto_bin\", \"bin_async_local_connect_timeout\", 5)\n";

/// Boot with no configured tree, so the built-in catalogue — and the
/// requested release — is what answers.
fn diagnostics_for(tag: &str, version: Option<&str>) -> Vec<String> {
    let base = std::env::temp_dir().join(format!("oslsp-vsel-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let cfg = base.join("v.cfg");
    std::fs::write(&cfg, CFG).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut opts = serde_json::json!({
        "opensipsPath": "",
        "opensipsSrc": "",
        "cacheDir": base.join("cache").display().to_string(),
    });
    if let Some(v) = version {
        opts["opensipsVersion"] = serde_json::json!(v);
    }

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_SRC", "")
            .env("OPENSIPS_LSP_VERSION", "")
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
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    // Wait for the catalogue to be in place first. Without this the
    // "no diagnostics" case below would be satisfied by a server that
    // simply had not got there yet — an empty result meaning nothing.
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":CFG}}}),
    );
    // Drain rather than block: a clean file may publish an empty list
    // or nothing at all, and "nothing" is a real answer here.
    let mut msgs: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(v) = rx.recv_timeout(left.min(std::time::Duration::from_millis(1500))) else {
            break;
        };
        if v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri {
            msgs = v["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["message"].as_str().map(str::to_string))
                .collect();
            break;
        }
    }
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
    msgs
}

#[test]
fn the_editor_checks_against_the_release_the_user_asked_for() {
    let newest = diagnostics_for("newest", None);
    let hit = newest
        .iter()
        .find(|m| m.contains("bin_async_local_connect_timeout"))
        .unwrap_or_else(|| {
            panic!("the newest release does not export it, so it must warn: {newest:?}")
        });
    assert!(
        hit.contains("4.0.1"),
        "the diagnostic must name the release it judged against: {hit:?}"
    );
    assert!(
        hit.contains("it exists in"),
        "and say where the name still exists: {hit:?}"
    );

    // the consequence: a release that really exports it is silent.
    // A test that only read the header would pass even if choosing a
    // release changed nothing about the answer.
    let older = diagnostics_for("older", Some("3.6.8"));
    assert!(
        !older
            .iter()
            .any(|m| m.contains("bin_async_local_connect_timeout")),
        "3.6.8 exports it, so the editor must not warn: {older:?}"
    );
}

/// An unsupported release must not quietly become the newest.
#[test]
fn an_unsupported_release_still_produces_a_working_server() {
    let msgs = diagnostics_for("bogus", Some("9.9.9"));
    let hit = msgs
        .iter()
        .find(|m| m.contains("bin_async_local_connect_timeout"))
        .unwrap_or_else(|| panic!("the fallback release must still check: {msgs:?}"));
    assert!(
        hit.contains("4.0.1"),
        "it falls back to the newest, and says so in the message: {hit:?}"
    );
}
