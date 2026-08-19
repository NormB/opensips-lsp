//! Diagnostics currency contract (adversarial analysis §2):
//!  - a failed or timed-out check CLEARS previously published
//!    diagnostics instead of leaving them pinned
//!  - every publish carries the document version of the buffer the
//!    mapping used
//!  - a check whose buffer changed mid-flight publishes nothing

mod common;
use common::*;
use std::process::{Command, Stdio};

/// Stub opensips: behavior keyed by a mode file next to the cfg.
///   error -> one positioned error;  hang -> sleep past the timeout
const STUB: &str = r#"#!/bin/sh
mode=$(cat "$(dirname "$3")/mode")
if [ "$mode" = hang ]; then sleep 30; fi
echo "CRITICAL:core:yyerror: parse error in $3:1:2-3: planted" >&2
exit 255
"#;

fn setup(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("oslsp-cur-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("stub.sh");
    std::fs::write(&stub, STUB).unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "route{}\n").unwrap();
    let uri = format!("file://{}", cfg.display());
    (dir, stub, uri)
}

#[test]
fn failed_checks_clear_stale_diagnostics_and_publishes_carry_versions() {
    let (dir, stub, uri) = setup("clear");
    std::fs::write(dir.join("mode"), "error").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", stub.display().to_string())
        .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "400")
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
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":7,"text":"route{}\n"}}}),
    );
    let d = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "first publish",
    );
    assert_eq!(d["params"]["diagnostics"].as_array().unwrap().len(), 1);
    // the publish must carry the version of the buffer used for mapping
    assert_eq!(d["params"]["version"], 7, "publish must be versioned: {d}");

    // now the checker hangs: the timeout must CLEAR the old diagnostic
    std::fs::write(dir.join("mode"), "hang").unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
        "textDocument":{"uri":uri}}}),
    );
    let d2 = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "clearing publish",
    );
    assert_eq!(
        d2["params"]["diagnostics"].as_array().unwrap().len(),
        0,
        "timeout must clear stale diagnostics: {d2}"
    );

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn midflight_buffer_change_suppresses_the_stale_result() {
    let (dir, stub, uri) = setup("race");
    std::fs::write(dir.join("mode"), "error").unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
        .env("OPENSIPS_LSP_BIN", stub.display().to_string())
        .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "5000")
        // make every check slow enough to race
        .env("OPENSIPS_LSP_TEST_CHECK_DELAY_MS", "700")
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
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );
    // edit while the check sleeps: its result must be suppressed
    std::thread::sleep(std::time::Duration::from_millis(150));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":"# changed\nroute{}\n"}]}}),
    );

    // drain: give the delayed check time to finish, then fence with a request
    std::thread::sleep(std::time::Duration::from_millis(1500));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"textDocument/hover","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":1}}}),
    );
    let mut saw_v1_publish = false;
    loop {
        let v = wait_for(&rx, |_| true, "fence");
        if v["method"] == "textDocument/publishDiagnostics" && v["params"]["version"] == 1 {
            saw_v1_publish = true;
        }
        if v["id"] == 9 {
            break;
        }
    }
    assert!(
        !saw_v1_publish,
        "a check raced by an edit must not publish for the old version"
    );

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}
