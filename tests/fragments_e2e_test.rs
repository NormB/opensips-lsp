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

/// The call graph, asked from inside a fragment.
///
/// `prepareCallHierarchy` on a route the PARENT defines yields an item
/// whose file is the root — a file the client has not opened, because
/// the user is editing the include.  Answering the follow-up from the
/// open-buffer map alone reads that file as empty, so the closure is
/// empty, so the call graph is empty: "nobody calls this" for a route
/// the file on screen calls two lines up.
#[test]
fn call_hierarchy_from_a_fragment_sees_the_whole_closure() {
    let (base, root_uri, frag_uri) = setup("callh");
    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/prepareCallHierarchy",
            "params":{"textDocument":{"uri":frag_uri},"position":{"line":1,"character":11}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "prepareCallHierarchy");
    let item = v["result"][0].clone();
    assert_eq!(
        item["data"]["route"], "helper",
        "the item is the route the PARENT defines: {v}"
    );
    assert_eq!(
        item["uri"], root_uri,
        "and it lives in the root, which is not open: {v}"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"callHierarchy/incomingCalls",
            "params":{"item": item}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "incomingCalls");
    let froms: Vec<&str> = v["result"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x["from"]["uri"].as_str()).collect())
        .unwrap_or_default();
    assert!(
        froms.contains(&frag_uri.as_str()),
        "the caller is the open fragment, two lines up: {v}"
    );
    let names: Vec<&str> = v["result"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["from"]["data"]["route"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(names.contains(&"entry"), "{v}");
    // and the other direction answers with a list rather than nothing
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"callHierarchy/outgoingCalls",
            "params":{"item": item}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "outgoingCalls");
    assert!(
        v["result"].is_array(),
        "a route whose body calls nothing has NO outgoing calls; that is \
         an empty list, not an unanswerable question: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A fragment the ROOT's closure cannot reach.
///
/// The closure is capped for safety — depth 8, 64 files — and a
/// deployment with one include per carrier passes 64 without trying.
/// Past the cap the fragment is not IN the closure built from the
/// root, and answering from that closure alone drops the fragment's
/// OWN includes: routes that were in scope before the root was ever
/// consulted start reading as undefined.  Analysing in the root's
/// context must only ever ADD to what the file could already see.
#[test]
fn a_fragment_outside_the_roots_closure_keeps_its_own_includes() {
    let base = std::env::temp_dir().join(format!("frag-wide-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    const N: usize = 90;
    let mut root = String::from("");
    for i in 0..N {
        root.push_str(&format!("include_file \"c{i}.cfg\"\n"));
    }
    root.push_str("route[TOP] {\n    exit;\n}\n");
    std::fs::write(base.join("opensips.cfg"), &root).unwrap();
    for i in 0..N - 1 {
        std::fs::write(
            base.join(format!("c{i}.cfg")),
            format!("route[C{i}] {{ exit; }}\n"),
        )
        .unwrap();
    }
    std::fs::write(
        base.join("helpers.cfg"),
        "route[OWN_HELPER] {\n    exit;\n}\n",
    )
    .unwrap();
    // the last sibling is past the cap, and includes a helper of its own
    let leaf_text = format!(
        "include_file \"helpers.cfg\"\nroute[C{}] {{\n    route(OWN_HELPER);\n    route(NOWHERE);\n}}\n",
        N - 1
    );
    let leaf = base.join(format!("c{}.cfg", N - 1));
    std::fs::write(&leaf, &leaf_text).unwrap();
    let leaf_uri = format!("file://{}", leaf.display());

    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &leaf_uri, &leaf_text);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == leaf_uri,
        "diagnostics on a fragment past the cap",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert!(
        !msgs.iter().any(|m| m.contains("OWN_HELPER")),
        "the fragment includes the file defining OWN_HELPER itself; the \
         root's closure being full must not take that away: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("NOWHERE")),
        "a route defined nowhere is still reported, so the absence \
         above is analysis and not silence: {msgs:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Differential: the same question, asked of the same text, laid out
/// two ways.
///
/// One workspace holds the whole configuration in one file; the other
/// holds exactly the same routes with the middle block moved into an
/// `include_file`.  Every positional request must answer the same
/// SHAPE in both — a fragment is a layout, not a different language.
///
/// This is the check that caught call hierarchy returning "nobody
/// calls this" from inside a fragment: every hand-written test agreed
/// with the code, and only asking the identical question of the
/// identical text in the other layout disagreed.
#[test]
fn every_request_answers_the_same_shape_split_as_whole() {
    let base = std::env::temp_dir().join(format!("frag-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let whole_dir = base.join("whole");
    let split_dir = base.join("split");
    std::fs::create_dir_all(&whole_dir).unwrap();
    std::fs::create_dir_all(split_dir.join("inc")).unwrap();

    let head = "";
    let whole = format!(
        "{head}route[helper] {{\n    exit;\n}}\nroute[entry] {{\n    route(helper);\n}}\nroute {{\n    route(entry);\n}}\n"
    );
    let root = format!(
        "{head}include_file \"inc/routes.cfg\"\nroute[helper] {{\n    exit;\n}}\nroute {{\n    route(entry);\n}}\n"
    );
    let frag = "route[entry] {\n    route(helper);\n}\n".to_string();
    std::fs::write(whole_dir.join("opensips.cfg"), &whole).unwrap();
    std::fs::write(split_dir.join("opensips.cfg"), &root).unwrap();
    std::fs::write(split_dir.join("inc/routes.cfg"), &frag).unwrap();
    let whole_uri = format!("file://{}", whole_dir.join("opensips.cfg").display());
    let frag_uri = format!("file://{}", split_dir.join("inc/routes.cfg").display());

    // the position of the `route(helper);` call, COMPUTED in each
    // layout: the two differ in header length, and a hardcoded line
    // silently turns every answer into null on one side
    let call_pos = |text: &str| -> (u32, u32) {
        let needle = "route(helper);";
        for (i, line) in text.lines().enumerate() {
            if let Some(col) = line.find(needle) {
                // INSIDE the route name.  Landing on the `)` instead
                // makes every request answer null on both sides, and
                // a differential test where both sides are null
                // passes while proving nothing.
                return (i as u32, (col + "route(".len() + 1) as u32);
            }
        }
        panic!("fixture has no {needle} call");
    };
    let (wl, wc) = call_pos(&whole);
    let (fl, fc) = call_pos(&frag);

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("opensips_LSP_BIN", "")
            .env("opensips_LSP_ANALYZER_DEBOUNCE_MS", "10")
            .env(
                "opensips_LSP_CACHE_DIR",
                base.join("cache").display().to_string(),
            )
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
            "capabilities":{},
            "initializationOptions":{"opensipsPath":""},
            "workspaceFolders":[
                {"uri": format!("file://{}", whole_dir.display()), "name":"whole"},
                {"uri": format!("file://{}", split_dir.display()), "name":"split"}]}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    did_open(&mut stdin, &whole_uri, &whole);
    did_open(&mut stdin, &frag_uri, &frag);

    /// What an answer IS, ignoring the paths — those differ by
    /// construction.  A null where the other side has an object is
    /// the disagreement worth failing on.
    fn shape(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Null => "null".into(),
            serde_json::Value::Array(a) => format!("array[{}]", a.len()),
            serde_json::Value::Object(o) => {
                let mut k: Vec<&str> = o.keys().map(|s| s.as_str()).collect();
                k.sort();
                format!("object[{}]", k.join(","))
            }
            other => format!("scalar[{other}]"),
        }
    }

    let mut answered = 0usize;
    let mut id = 10;
    let mut ask = |uri: &str, line: u32, ch: u32, method: &str, extra: serde_json::Value| {
        id += 1;
        let mut params = serde_json::json!({
            "textDocument": {"uri": uri},
            "position": {"line": line, "character": ch}
        });
        if let Some(o) = extra.as_object() {
            for (k, v) in o {
                params[k] = v.clone();
            }
        }
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        );
        let want = id;
        wait_for(&rx, |v| v["id"] == want, method)["result"].clone()
    };

    for (method, extra) in [
        ("textDocument/hover", serde_json::json!({})),
        ("textDocument/definition", serde_json::json!({})),
        (
            "textDocument/references",
            serde_json::json!({"context": {"includeDeclaration": true}}),
        ),
        ("textDocument/prepareRename", serde_json::json!({})),
        ("textDocument/rename", serde_json::json!({"newName": "ZZZ"})),
        ("textDocument/signatureHelp", serde_json::json!({})),
        ("textDocument/prepareCallHierarchy", serde_json::json!({})),
    ] {
        let w = ask(&whole_uri, wl, wc, method, extra.clone());
        let f = ask(&frag_uri, fl, fc, method, extra.clone());
        // `null == null` would agree while proving nothing; at least
        // one request has to have actually answered
        if !w.is_null() {
            answered += 1;
        }
        assert_eq!(
            shape(&w),
            shape(&f),
            "{method} answers differently once the block is an include\n  whole: {w}\n  split: {f}"
        );
    }

    assert!(
        answered >= 5,
        "only {answered} of the positional requests answered at all — the \
         cursor is not on a route name and the comparison is vacuous"
    );

    // the call-hierarchy follow-ups, which is where the difference was
    let mut w_shapes: Vec<String> = Vec::new();
    let mut f_shapes: Vec<String> = Vec::new();
    let w_item = ask(
        &whole_uri,
        wl,
        wc,
        "textDocument/prepareCallHierarchy",
        serde_json::json!({}),
    )[0]
    .clone();
    let f_item = ask(
        &frag_uri,
        fl,
        fc,
        "textDocument/prepareCallHierarchy",
        serde_json::json!({}),
    )[0]
    .clone();
    for method in ["callHierarchy/incomingCalls", "callHierarchy/outgoingCalls"] {
        for (label, item) in [("whole", &w_item), ("split", &f_item)] {
            id += 1;
            write_msg(
                &mut stdin,
                &serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,
                    "params":{"item": item}}),
            );
            let want = id;
            let r = wait_for(&rx, |v| v["id"] == want, method)["result"].clone();
            if label == "whole" {
                w_shapes.push(shape(&r));
            } else {
                f_shapes.push(shape(&r));
            }
        }
    }
    assert_eq!(
        w_shapes, f_shapes,
        "the call graph must not depend on which file a route lives in"
    );

    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Hostile and malformed URIs to `opensips/analysisRoot`.
///
/// It is answered before a document has a language, which means it is
/// answered for whatever the editor happens to have open — and an
/// editor will hand over `untitled:`, a directory, a percent-encoded
/// NUL, a path that climbs above the workspace.  A panic in a request
/// this early takes the whole server down for every open file.
#[test]
fn analysis_root_survives_whatever_the_editor_hands_it() {
    let (base, _root_uri, frag_uri) = setup("hostile");
    let (mut child, rx, mut stdin) = start(&base, "");
    let dir = base.display().to_string();
    let hostile = [
        String::new(),
        "not-a-uri".into(),
        "http://example.com/x.cfg".into(),
        "file://".into(),
        "file:///".into(),
        "untitled:Untitled-1".into(),
        format!("file://{dir}"),
        format!("file://{dir}/does-not-exist.cfg"),
        format!("file://{dir}/inc/routes.cfg/"),
        format!("file://{dir}/../{}", "x".repeat(64)),
        "file:///%00".into(),
        "file:///%2e%2e/%2e%2e/etc/passwd".into(),
        format!("file://{dir}/{}.cfg", "deep/".repeat(40)),
    ];
    let mut id = 100;
    for uri in &hostile {
        id += 1;
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
                "params":{"uri": uri}}),
        );
        let want = id;
        // an answer OR a protocol error is fine; silence is not
        let v = wait_for(&rx, |v| v["id"] == want, "analysisRoot on a hostile uri");
        assert!(
            v.get("result").is_some() || v.get("error").is_some(),
            "neither answered nor refused {uri:?}: {v}"
        );
    }
    // and it is still correct afterwards, so nothing was left wedged
    id += 1;
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let want = id;
    let v = wait_for(
        &rx,
        |v| v["id"] == want,
        "analysisRoot after the hostile batch",
    );
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "the server must still answer correctly after that: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// The two diagnostic channels must not disagree about a fragment.
///
/// A client that pulls is answered by `textDocument/diagnostic`; one
/// that does not is pushed to.  They are separate code paths with
/// separate call sites for the analysis root, so a fragment is
/// exactly the shape of thing that ends up analysed one way on one
/// channel and another way on the other.
#[test]
fn pull_and_push_agree_about_a_fragment() {
    let (base, root_uri, frag_uri) = setup("channels");

    // push: a client that declares no diagnostic support
    let (mut pusher, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "pushed diagnostics",
    );
    let mut pushed: Vec<String> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str().map(str::to_string))
        .collect();
    pushed.sort();
    let _ = pusher.kill();

    // pull: the same workspace, a client that declares it
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("opensips_LSP_BIN", "")
            .env("opensips_LSP_ANALYZER_DEBOUNCE_MS", "10")
            .env(
                "opensips_LSP_CACHE_DIR",
                base.join("cache2").display().to_string(),
            )
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
            "capabilities":{"textDocument":{"diagnostic":{"dynamicRegistration":false}}},
            "initializationOptions":{"opensipsPath":""},
            "workspaceFolders":[{"uri": format!("file://{}", base.display()), "name":"w"}]}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/diagnostic",
            "params":{"textDocument":{"uri":frag_uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "pulled diagnostics");
    let mut pulled: Vec<String> = v["result"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|d| d["message"].as_str().map(str::to_string))
        .collect();
    pulled.sort();
    assert!(!pushed.is_empty(), "the fixture must produce something");
    assert_eq!(pushed, pulled, "the two channels disagree about a fragment");

    // and the workspace sweep reports the ROOT, never the fragment
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"workspace/diagnostic",
            "params":{"previousResultIds":[]}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "workspace sweep");
    let uris: Vec<String> = v["result"]["items"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| i["uri"].as_str().map(str::to_string))
        .collect();
    assert!(uris.contains(&root_uri), "the root is a program: {uris:?}");
    assert!(
        !uris.contains(&frag_uri),
        "a fragment is not a program and must not be swept: {uris:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Many fragments open at once, edited in a burst.
///
/// The include graph is a single cached structure behind an
/// `RwLock`, invalidated from didOpen, didChange, didClose and the
/// watcher.  Rebuilding it while another request reads it is exactly
/// the shape of thing that deadlocks or serves a half-built answer,
/// and it would do so only under load — never in a test that opens
/// one file.
#[test]
fn many_fragments_open_and_edited_at_once_stay_correct() {
    let base = std::env::temp_dir().join(format!("frag-load-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();
    const N: usize = 12;
    let mut root = String::new();
    for i in 0..N {
        root.push_str(&format!("include_file \"inc/f{i}.cfg\"\n"));
    }
    root.push_str("route[SHARED] {\n    exit;\n}\n");
    std::fs::write(base.join("opensips.cfg"), &root).unwrap();
    let text = |i: usize| format!("route[F{i}] {{\n    route(SHARED);\n    route(ghost);\n}}\n");
    for i in 0..N {
        std::fs::write(base.join(format!("inc/f{i}.cfg")), text(i)).unwrap();
    }
    let (mut child, rx, mut stdin) = start(&base, "");
    let uri = |i: usize| format!("file://{}", base.join(format!("inc/f{i}.cfg")).display());
    for i in 0..N {
        did_open(&mut stdin, &uri(i), &text(i));
    }
    // a burst of edits across all of them, interleaved
    for round in 2..5 {
        for i in 0..N {
            write_msg(
                &mut stdin,
                &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                    "textDocument":{"uri": uri(i), "version": round},
                    "contentChanges":[{"text": format!("{}// r{round}\n", text(i))}]}}),
            );
        }
    }
    // every one of them must still answer, and answer correctly
    let mut id = 200;
    for i in 0..N {
        id += 1;
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
                "params":{"uri": uri(i)}}),
        );
        let want = id;
        let v = wait_for(&rx, |v| v["id"] == want, "analysisRoot under load");
        assert!(
            v["result"]
                .as_str()
                .is_some_and(|s| s.ends_with("opensips.cfg")),
            "fragment {i} lost its root under load: {v}"
        );
    }
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}
