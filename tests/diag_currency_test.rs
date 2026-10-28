//! Diagnostics currency contract (adversarial analysis §2):
//!  - a failed or timed-out check CLEARS previously published
//!    diagnostics instead of leaving them pinned
//!  - every publish carries the document version of the buffer the
//!    mapping used
//!  - a check whose buffer changed mid-flight publishes nothing

mod common;
use common::Server;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Stub opensips: behavior keyed by a mode file next to the cfg.
///   error -> one positioned error;  hang -> sleep past the timeout;
///   slow -> take six seconds, inside the timeout
const STUB: &str = r#"#!/bin/sh
for a in "$@"; do cfg="$a"; done
mode=$(cat "$(dirname "$cfg")/mode")
if [ "$mode" = hang ]; then sleep 30; fi
if [ "$mode" = slow ]; then sleep 6; fi
echo "CRITICAL:core:yyerror: parse error in $cfg:1:2-3: planted" >&2
exit 255
"#;

fn setup(tag: &str) -> (common::Scratch, PathBuf, PathBuf, String) {
    let dir_scratch = common::Scratch::new(&format!("oslsp-cur-{tag}"));
    let dir = dir_scratch.path().to_path_buf();
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
    (dir_scratch, dir, stub, uri)
}

#[test]
fn failed_checks_clear_stale_diagnostics_and_publishes_carry_versions() {
    let (_scratch, dir, stub, uri) = setup("clear");
    std::fs::write(dir.join("mode"), "error").unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            // long enough for the "error" check to finish on a loaded host;
            // the "hang" check sleeps 30 s, so it still times out
            .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "3000")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":7,"text":"route{}\n"}}}),
    );
    let d = common::wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "first publish",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().unwrap().len(),
        1,
        "the error check reports its one planted error (none means it timed out): {d}"
    );
    // the publish must carry the version of the buffer used for mapping
    assert_eq!(d["params"]["version"], 7, "publish must be versioned: {d}");

    // now the checker hangs: the timeout must CLEAR the old diagnostic
    std::fs::write(dir.join("mode"), "hang").unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didSave","params":{
        "textDocument":{"uri":uri}}}),
    );
    let d2 = common::wait_for(
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
}

#[test]
fn midflight_buffer_change_suppresses_the_stale_result() {
    let (_scratch, dir, stub, uri) = setup("race");
    std::fs::write(dir.join("mode"), "error").unwrap();

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "5000")
            // make every check slow enough to race
            .env("OPENSIPS_LSP_TEST_CHECK_DELAY_MS", "700")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );
    // edit while the check sleeps: its result must be suppressed
    std::thread::sleep(Duration::from_millis(150));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":"# changed\nroute{}\n"}]}}),
    );

    // drain: give the delayed check time to finish, then fence with a request
    std::thread::sleep(Duration::from_millis(1500));
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"textDocument/hover","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":1}}}),
    );
    let mut saw_v1_publish = false;
    loop {
        let v = common::wait_for(&rx, |_| true, "fence");
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
}

#[test]
fn output_flood_is_capped_and_clears_diagnostics() {
    let dir_scratch = common::Scratch::new("oslsp-flood");
    let dir = dir_scratch.path().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let stub = dir.join("flood.sh");
    // ~8 MiB of noise, then a "valid" error line that must never be seen
    std::fs::write(
        &stub,
        "#!/bin/sh\nfor a in \"$@\"; do cfg=\"$a\"; done\nhead -c 8388608 /dev/zero | tr '\\0' 'A' >&2\necho \"CRITICAL:core:yyerror: parse error in $cfg:1:1-2: after flood\" >&2\nexit 255\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut p = std::fs::metadata(&stub).unwrap().permissions();
    p.set_mode(0o755);
    std::fs::set_permissions(&stub, p).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, "route{}\n").unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_OUTPUT_CAP_BYTES", "65536")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":"route{}\n"}}}),
    );

    // the flood must be reported via logMessage and the publish must be EMPTY
    let log = common::wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("output cap")
        },
        "cap logMessage",
    );
    assert!(
        log["params"]["message"]
            .as_str()
            .unwrap()
            .contains("output cap"),
        "the flood must be reported via logMessage: {log}"
    );
    let d = common::wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics",
        "publish",
    );
    assert_eq!(
        d["params"]["diagnostics"].as_array().unwrap().len(),
        0,
        "capped run must clear, not publish flood-derived results: {d}"
    );
    child.kill().ok();
}

/// A check runs a subprocess and can take seconds; opening a document
/// must not hold up the requests that come after it.
///
/// The transport runs at most four messages at once, so a `didOpen`
/// that waits for its check occupies a slot for the check's whole run.
/// Five files opened together used to leave a request queued behind
/// them until two checks had finished.
#[test]
fn requests_are_answered_while_checks_run() {
    let (_scratch, dir, stub, _) = setup("busy");
    std::fs::write(dir.join("mode"), "slow").unwrap();
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", stub.display().to_string())
            .env("OPENSIPS_LSP_CHECK_TIMEOUT_MS", "20000")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "init");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let mut uris = Vec::new();
    for i in 0..5 {
        let cfg = dir.join(format!("f{i}.cfg"));
        let text = "route[r] {\n    exit;\n}\n";
        std::fs::write(&cfg, text).unwrap();
        let uri = format!("file://{}", cfg.display());
        common::write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
                "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
        );
        uris.push(uri);
    }
    let t = Instant::now();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/documentSymbol",
            "params":{"textDocument":{"uri":uris[0]}}}),
    );
    let r = common::wait_for(&rx, |v| v["id"] == 2, "documentSymbol");
    let took = t.elapsed();
    assert_eq!(r["result"].as_array().map(|a| a.len()), Some(1), "{r}");
    assert!(
        // a request queued behind a check waits its six seconds
        took < Duration::from_secs(5),
        "documentSymbol waited {took:?} behind the checks"
    );
    // the checks still run and still publish
    for _ in 0..5 {
        common::wait_for(
            &rx,
            |v| v["method"] == "textDocument/publishDiagnostics",
            "a check result",
        );
    }
    child.kill().ok();
}
