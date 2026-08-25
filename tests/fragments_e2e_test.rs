//! An included FRAGMENT opened on its own.
//!
//! A config another config includes is not a program: analysed alone
//! it flags every route its parent defines as undefined and hands the
//! checker something `opensips -C` was never meant to accept.  The
//! workspace sweep has always known this and skipped fragments; the
//! single-document path did not, so opening one directly produced
//! errors that were artefacts of how it was opened.
//!
//! These tests open ONLY the fragment and require the server to
//! answer in its root's context.

mod common;
use common::*;
use std::process::{Command, Stdio};

/// Root defines `helper` and includes the fragment; the fragment
/// defines `entry`, calls `helper` (defined only in the root) and
/// calls `ghost` (defined nowhere).
const ROOT_TEXT: &str = "include_file \"inc/routes.cfg\"\nroute[helper] {\n    exit;\n}\nroute {\n    route(entry);\n}\n";
const FRAG_TEXT: &str = "route[entry] {\n    route(helper);\n    route(ghost);\n}\n";

fn setup(tag: &str) -> (std::path::PathBuf, String, String) {
    let base = std::env::temp_dir().join(format!("oslsp-frag-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();
    std::fs::write(base.join("opensips.cfg"), ROOT_TEXT).unwrap();
    std::fs::write(base.join("inc/routes.cfg"), FRAG_TEXT).unwrap();
    let root_uri = format!("file://{}", base.join("opensips.cfg").display());
    let frag_uri = format!("file://{}", base.join("inc/routes.cfg").display());
    (base, root_uri, frag_uri)
}

fn start(
    base: &std::path::Path,
    opensips_path: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
) {
    start_in(base, opensips_path, true)
}

fn start_in(
    base: &std::path::Path,
    opensips_path: &str,
    with_workspace_folder: bool,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
) {
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_ANALYZER_DEBOUNCE_MS", "10")
            .env(
                "OPENSIPS_LSP_CACHE_DIR",
                base.join("cache").display().to_string(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        base,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    let folders = with_workspace_folder
        .then(|| serde_json::json!([{"uri": format!("file://{}", base.display()), "name":"w"}]));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{},
            "initializationOptions":{"opensipsPath": opensips_path},
            "workspaceFolders": folders}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin)
}

fn did_open(stdin: &mut std::process::ChildStdin, uri: &str, text: &str) {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
}

#[test]
fn the_server_names_the_root_a_fragment_belongs_to() {
    let (base, root_uri, frag_uri) = setup("root");
    let (mut child, rx, mut stdin) = start(&base, "");
    // the fragment's context is the root that includes it...
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot(fragment)");
    assert_eq!(
        v["result"], root_uri,
        "the fragment must resolve to its including root: {v}"
    );
    // ...and a config nothing includes is a program in its own right,
    // which is how the client tells "opensips config" from "some .cfg"
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"opensips/analysisRoot",
            "params":{"uri": root_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "analysisRoot(root)");
    assert!(v["result"].is_null(), "a root has no parent: {v}");
    // a file the workspace does not contain is not claimed
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"opensips/analysisRoot",
            "params":{"uri": format!("file://{}", base.join("nope.cfg").display())}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "analysisRoot(unknown)");
    assert!(v["result"].is_null(), "{v}");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_fragment_opened_alone_is_analysed_in_its_roots_closure() {
    let (base, _root_uri, frag_uri) = setup("analyze");
    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "fragment diagnostics",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["message"].as_str().unwrap())
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("ghost")),
        "a genuinely undefined route is still reported: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("helper")),
        "helper is defined in the root that includes this fragment; \
         flagging it is an artefact of analysing the fragment alone: {msgs:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn navigation_from_a_fragment_reaches_the_root_that_includes_it() {
    let (base, root_uri, frag_uri) = setup("nav");
    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    // definition of `helper` (fragment line 1) lives in the root
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
            "textDocument":{"uri":frag_uri},"position":{"line":1,"character":11}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "definition");
    assert_eq!(
        v["result"]["uri"], root_uri,
        "the parent's route is only reachable through the root closure: {v}"
    );
    assert_eq!(v["result"]["range"]["start"]["line"], 1);
    // completion inside `route(` offers the parent's routes too
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
            "textDocument":{"uri":frag_uri},"position":{"line":2,"character":10}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "completion");
    let labels: Vec<&str> = v["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(labels.contains(&"entry"), "own route missing: {labels:?}");
    assert!(
        labels.contains(&"helper"),
        "the root's route must be offered: {labels:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_checker_runs_on_the_root_and_the_fragments_errors_come_back_to_it() {
    let (base, _root_uri, frag_uri) = setup("check");
    // Stub checker: records argv, then reports one error located in
    // the FRAGMENT, spelled the way opensips spells an include — the
    // path as written in the directive.
    let argv_file = base.join("argv");
    let stub = base.join("stub.sh");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\necho \"CRITICAL:core:yyerror: parse error in inc/routes.cfg:3:5-9: planted error\" >&2\nexit 255\n",
            argv_file.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let (mut child, rx, mut stdin) = start(&base, stub.to_str().unwrap());
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == frag_uri
                && v["params"]["diagnostics"]
                    .as_array()
                    .is_some_and(|a| a.iter().any(|d| d["source"] == "opensips -C"))
        },
        "checker diagnostics on the fragment",
    );
    let planted = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["source"] == "opensips -C")
        .cloned()
        .unwrap();
    assert!(
        planted["message"]
            .as_str()
            .unwrap()
            .contains("planted error"),
        "{planted}"
    );
    assert_eq!(
        planted["range"]["start"]["line"], 2,
        "at the fragment's own line, not folded onto an include \
         directive it does not contain: {planted}"
    );
    // the checker was handed the ROOT: a fragment is not a program
    let argv = std::fs::read_to_string(&argv_file).unwrap();
    let args: Vec<&str> = argv.lines().collect();
    assert!(
        args.iter().any(|a| a.ends_with("opensips.cfg")),
        "the root must be the file checked: {args:?}"
    );
    assert!(
        !args.iter().any(|a| a.ends_with("routes.cfg")),
        "the fragment must not be handed to the checker: {args:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// The root is found by reading the configs under the workspace
/// folders, so a client that opened one FILE and no folder has given
/// the server nothing to read.  There is no answer to invent there —
/// the root may be any directory above — and the getting-started
/// guide tells the reader to open the folder for this reason.  If
/// that ever stops being true, this gate is what says the page needs
/// rewriting.
#[test]
fn without_a_workspace_folder_there_is_nothing_to_find_the_root_in() {
    let (base, _root_uri, frag_uri) = setup("nows");
    let (mut child, rx, mut stdin) = start_in(&base, "", false);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot(no folder)");
    assert!(
        v["result"].is_null(),
        "with no folder open the include graph is empty: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Adding the `include_file` line is the FIX for a fragment that was
/// reporting its parent's routes as undefined.  The warning has to go
/// when the fix is typed, not when the fragment is next touched —
/// otherwise the fix looks like it did not work, and the next thing
/// the user tries is something that will not help.
#[test]
fn adding_an_include_clears_the_fragment_it_now_belongs_to() {
    let (base, root_uri, frag_uri) = setup("live");
    // start from a root that does NOT include the fragment
    let root_without = ROOT_TEXT.replace("include_file \"inc/routes.cfg\"\n", "");
    std::fs::write(base.join("opensips.cfg"), &root_without).unwrap();
    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "fragment diagnostics before the include exists",
    );
    assert!(
        v["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["message"].as_str().is_some_and(|m| m.contains("helper"))),
        "nothing includes it yet, so its parent's route really is \
         undefined here: {v}"
    );
    // the user opens the root and types the include line
    did_open(&mut stdin, &root_uri, &root_without);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":root_uri,"version":2},
            "contentChanges":[{"text": ROOT_TEXT}]}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == frag_uri
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|d| d["message"].as_str().is_some_and(|m| m.contains("helper")))
        },
        "the fragment must be re-published once its root includes it",
    );
    // the route that is genuinely undefined is still reported, so the
    // clearing above is a re-analysis and not a wipe
    assert!(
        v["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["message"]
                .as_str()
                .is_some_and(|m| m.contains("GHOST") || m.contains("ghost"))),
        "{v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// The include graph is built from a BOUNDED scan of the workspace.
/// Past the bound a root simply is not seen, and the fragment stops
/// being recognised: no colours, no context, nothing on screen and —
/// until this gate — nothing in the log either.  A cap the user
/// cannot see is a cap they cannot work around.
#[test]
fn a_truncated_workspace_scan_says_so() {
    let (base, _root_uri, frag_uri) = setup("cap");
    let noise = base.join("noise");
    std::fs::create_dir_all(&noise).unwrap();
    for i in 0..520 {
        std::fs::write(noise.join(format!("n{i:04}.cfg")), "# unrelated\n").unwrap();
    }
    let (mut child, rx, mut stdin) = start(&base, "");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("500") && m.contains("include"))
        },
        "a log line naming the scan bound",
    );
    let msg = v["params"]["message"].as_str().unwrap();
    assert!(
        msg.contains("opensips-lsp"),
        "the line must say who is speaking: {msg}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}
