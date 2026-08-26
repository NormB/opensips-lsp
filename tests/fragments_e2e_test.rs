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

/// One definition of "a config this server can read".
///
/// The workspace scan behind the include graph and the loader behind
/// the include closure each decided for themselves, and they
/// disagreed twice over.  A root larger than the loader's cap was
/// FOUND by the scan and then could not be loaded, so its routes
/// silently left scope while the fragment still claimed it as a root.
/// A root with one byte that is not UTF-8 — a latin-1 accent in a
/// comment, which is what a Spanish or German deployment looks like —
/// was dropped from the scan entirely, taking every fragment it
/// includes with it.
#[test]
fn a_config_the_scan_accepts_is_a_config_the_closure_can_load() {
    let base = std::env::temp_dir().join(format!("frag-read-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();
    let frag = "route[F] {\n    route(TOP);\n    route(NOWHERE);\n}\n";
    std::fs::write(base.join("inc/f.cfg"), frag).unwrap();
    let frag_uri = format!("file://{}", base.join("inc/f.cfg").display());
    let root_path = base.join("opensips.cfg");
    let root = "include_file \"inc/f.cfg\"\nroute[TOP] {\n    exit;\n}\n";

    // a comment carrying one byte that is not valid UTF-8
    let mut bytes = root.as_bytes().to_vec();
    bytes.extend_from_slice(b"# marca de se\xf1al\n");
    std::fs::write(&root_path, &bytes).unwrap();
    assert!(
        String::from_utf8(bytes.clone()).is_err(),
        "the fixture must be invalid UTF-8"
    );

    let (mut child, rx, mut stdin) = start(&base, "");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot with a non-UTF-8 root");
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "one byte in a comment must not erase the whole configuration: {v}"
    );
    did_open(&mut stdin, &frag_uri, frag);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "diagnostics with a non-UTF-8 root",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert!(
        !msgs.iter().any(|m| m.contains("TOP")),
        "the root's routes are in scope despite the byte: {msgs:?}"
    );
    assert!(
        msgs.iter().any(|m| m.contains("NOWHERE")),
        "and the analyzer is still running: {msgs:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A config too large to load is too large to claim, and says so.
///
/// The two components must agree: if the closure will not load a
/// file, the graph must not hand out a root that depends on it —
/// "found, but its routes are not in scope" is the worst of both.
#[test]
fn a_root_too_large_to_load_is_not_claimed_and_is_announced() {
    let base = std::env::temp_dir().join(format!("frag-huge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();
    let frag = "route[F] {\n    route(TOP);\n}\n";
    std::fs::write(base.join("inc/f.cfg"), frag).unwrap();
    let frag_uri = format!("file://{}", base.join("inc/f.cfg").display());
    let mut root = String::from("include_file \"inc/f.cfg\"\nroute[TOP] {\n    exit;\n}\n");
    while root.len() <= 1_048_576 {
        root.push_str("# pad pad pad pad pad pad pad pad pad pad pad pad\n");
    }
    std::fs::write(base.join("opensips.cfg"), &root).unwrap();

    let (mut child, rx, mut stdin) = start(&base, "");
    // the first request is what builds the graph; the log and the
    // response race, and `wait_for` discards what it is not waiting
    // for — so take the log first and ask again for the answer
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
                    .is_some_and(|m| m.contains("1 MiB") || m.contains("could not be read"))
        },
        "a log line explaining the skipped config",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let answer = wait_for(&rx, |v| v["id"] == 3, "analysisRoot with an oversized root");
    assert!(
        answer["result"].is_null(),
        "a root the closure cannot load must not be handed out: {answer}"
    );
    assert!(
        v["params"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("opensips-lsp")),
        "the line must say who is speaking: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A fixture that is not the text it looks like passes while proving
/// nothing.
///
/// Three tests in this file once went green against a "config" whose
/// first two lines were joined by a literal backslash-n: the script
/// that generated them quoted `\n` one level too many, so `#!KAMAILIO`
/// and the route definition below it were one line, the route was
/// never defined, and every request answered null on both sides of a
/// comparison that then agreed.  Nothing else in this file needs a
/// literal backslash-n, so its presence is the mistake.
#[test]
fn no_fixture_here_carries_a_literal_backslash_n() {
    // built rather than written, or this line matches itself
    let bs = '\\';
    let needle = format!("{bs}{bs}n");
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests");
    let mut checked = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        checked += 1;
        for (i, line) in src.lines().enumerate() {
            // a shell stub needs a backslash-n of its own for `printf`
            if line.contains("#!/bin/sh") || line.contains("{bs}{bs}n") {
                continue;
            }
            let bytes = line.as_bytes();
            for (at, _) in line.match_indices(&needle) {
                // `\\` before a real newline is a CONFIG line
                // continuation and legitimate; the mistake is a
                // backslash-n standing on its own
                if at > 0 && bytes[at - 1] == b'\\' {
                    continue;
                }
                panic!(
                    "{}:{}: a literal backslash-n where a newline was meant:\n  {line}",
                    path.display(),
                    i + 1
                );
            }
        }
    }
    assert!(checked > 5, "only {checked} test files scanned");
}

/// Line endings, byte-order marks and multibyte names.
///
/// The include path, the route name and the reported column all come
/// from byte offsets into text the editor supplies, and CRLF, a BOM
/// and a multibyte identifier each shift those offsets differently.
#[test]
fn encodings_do_not_change_what_a_fragment_is_part_of() {
    for (tag, eol, bom) in [
        ("lf", "\n", ""),
        ("crlf", "\r\n", ""),
        ("bom", "\n", "\u{feff}"),
    ] {
        let base = std::env::temp_dir().join(format!("frag-enc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).unwrap();
        let root =
            format!("{bom}include_file \"inc/f.cfg\"{eol}route[TOP] {{{eol}    exit;{eol}}}{eol}");
        let frag =
            format!("{bom}route[F] {{{eol}    route(TOP);{eol}    route(NOWHERE);{eol}}}{eol}");
        std::fs::write(base.join("opensips.cfg"), &root).unwrap();
        std::fs::write(base.join("inc/f.cfg"), &frag).unwrap();
        let frag_uri = format!("file://{}", base.join("inc/f.cfg").display());
        let (mut child, rx, mut stdin) = start(&base, "");
        did_open(&mut stdin, &frag_uri, &frag);
        let v = wait_for(
            &rx,
            |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
            "diagnostics",
        );
        let items = v["params"]["diagnostics"].as_array().unwrap().clone();
        let msgs: Vec<&str> = items.iter().filter_map(|d| d["message"].as_str()).collect();
        // A BOM is not whitespace, so it is not skipped before the
        // directive and `cfg_pp.c` never matches `include_file` on
        // that line: OpenSIPS really does not fire the include, and
        // `route(TOP)` really is undefined. Verified against the
        // pinned binary — the same config opens the file without the
        // BOM and does not open it with one. The server reporting it
        // is the server agreeing with the parser, and hiding it would
        // hide a config that is silently not including anything.
        if bom.is_empty() {
            assert!(!msgs.iter().any(|m| m.contains("TOP")), "{tag}: {msgs:?}");
        } else {
            assert!(
                msgs.iter().any(|m| m.contains("TOP")),
                "{tag}: a BOM suppresses the include, so TOP is genuinely undefined: {msgs:?}"
            );
        }
        assert!(
            msgs.iter().any(|m| m.contains("NOWHERE")),
            "{tag}: {msgs:?}"
        );
        // the reported range must frame the name it names, whatever
        // the line ending did to the offsets
        let d = items
            .iter()
            .find(|d| d["message"].as_str().is_some_and(|m| m.contains("NOWHERE")))
            .unwrap();
        let line = frag
            .lines()
            .nth(d["range"]["start"]["line"].as_u64().unwrap() as usize)
            .unwrap();
        let s = d["range"]["start"]["character"].as_u64().unwrap() as usize;
        let e = d["range"]["end"]["character"].as_u64().unwrap() as usize;
        let slice: String = line.chars().skip(s).take(e - s).collect();
        assert_eq!(slice, "NOWHERE", "{tag}: the range does not frame the name");
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&base);
    }
}

/// What the filesystem can be that a config file is not.
#[test]
fn the_filesystem_being_hostile_does_not_take_the_server_down() {
    let base = std::env::temp_dir().join(format!("frag-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();
    // the include target is a DIRECTORY that happens to be named .cfg
    std::fs::create_dir_all(base.join("inc/dir.cfg")).unwrap();
    // and a symlink pointing at a real fragment
    std::fs::write(base.join("real.cfg"), "route[S] {\n    exit;\n}\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(base.join("real.cfg"), base.join("inc/link.cfg")).unwrap();
    // and a directory symlink loop for the scan to walk into
    #[cfg(unix)]
    std::os::unix::fs::symlink(&base, base.join("inc/loop")).unwrap();
    std::fs::write(base.join("inc/f.cfg"), "route[F] {\n    exit;\n}\n").unwrap();
    std::fs::write(
        base.join("opensips.cfg"),
        "include_file \"inc/dir.cfg\"\ninclude_file \"inc/link.cfg\"\ninclude_file \"inc/f.cfg\"\nroute[TOP] {\n    exit;\n}\n",
    )
    .unwrap();
    let (mut child, rx, mut stdin) = start(&base, "");
    for name in [
        "inc/dir.cfg",
        "inc/link.cfg",
        "inc/f.cfg",
        "real.cfg",
        "inc/loop",
    ] {
        let uri = format!("file://{}", base.join(name).display());
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":50,"method":"opensips/analysisRoot",
                "params":{"uri": uri}}),
        );
        // a directory symlink loop must not make the scan run forever
        let v = wait_for(&rx, |v| v["id"] == 50, "analysisRoot on a hostile path");
        assert!(
            v.get("result").is_some() || v.get("error").is_some(),
            "{name}: {v}"
        );
    }
    // the ordinary fragment among them still resolves
    let f_uri = format!("file://{}", base.join("inc/f.cfg").display());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":51,"method":"opensips/analysisRoot",
            "params":{"uri": f_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 51, "analysisRoot");
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "a real fragment beside hostile ones still resolves: {v}"
    );
    // and the root vanishing mid-session is survivable
    std::fs::remove_file(base.join("opensips.cfg")).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{
            "changes":[{"uri": format!("file://{}", base.join("opensips.cfg").display()), "type":3}]}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":52,"method":"opensips/analysisRoot",
            "params":{"uri": f_uri}}),
    );
    let v = wait_for(
        &rx,
        |v| v["id"] == 52,
        "analysisRoot after the root was deleted",
    );
    assert!(v["result"].is_null(), "the root is gone: {v}");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Metamorphic: the same routes, whole and split, must be analysed
/// identically.
///
/// Not "the shapes match" but "the findings match" — the same set of
/// messages, for the same text, however it is distributed across
/// files.  Splitting a configuration is a layout choice and must not
/// be a semantic one.
#[test]
fn splitting_a_configuration_does_not_change_the_findings() {
    // every block carries a finding of its own: a clean fragment
    // publishes nothing, and a test that waits for that publish waits
    // forever
    let blocks = [
        "route[A] {\n    route(B);\n    route(MISSING_A);\n}\n",
        "route[B] {\n    route(C);\n    route(MISSING_B);\n}\n",
        "route[C] {\n    route(A);\n    route(MISSING_C);\n}\n",
    ];
    let tail = "route {\n    route(A);\n}\n";

    let base = std::env::temp_dir().join(format!("frag-meta-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let whole_dir = base.join("whole");
    let split_dir = base.join("split");
    std::fs::create_dir_all(&whole_dir).unwrap();
    std::fs::create_dir_all(split_dir.join("inc")).unwrap();
    let whole = format!("{}{tail}", blocks.concat());
    std::fs::write(whole_dir.join("opensips.cfg"), &whole).unwrap();
    let mut root = String::from("");
    for i in 0..blocks.len() {
        root.push_str(&format!("include_file \"inc/b{i}.cfg\"\n"));
    }
    root.push_str(tail);
    std::fs::write(split_dir.join("opensips.cfg"), &root).unwrap();
    for (i, b) in blocks.iter().enumerate() {
        std::fs::write(split_dir.join(format!("inc/b{i}.cfg")), b).unwrap();
    }

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
                {"uri": format!("file://{}", whole_dir.display()), "name":"w"},
                {"uri": format!("file://{}", split_dir.display()), "name":"s"}]}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );

    let whole_uri = format!("file://{}", whole_dir.join("opensips.cfg").display());
    did_open(&mut stdin, &whole_uri, &whole);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == whole_uri,
        "whole-file diagnostics",
    );
    let mut from_whole: Vec<String> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str().map(str::to_string))
        .collect();
    from_whole.sort();

    let mut from_split: Vec<String> = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let uri = format!(
            "file://{}",
            split_dir.join(format!("inc/b{i}.cfg")).display()
        );
        did_open(&mut stdin, &uri, b);
        let want = uri.clone();
        let v = wait_for(
            &rx,
            move |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == want,
            "fragment diagnostics",
        );
        from_split.extend(
            v["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|d| d["message"].as_str().map(str::to_string)),
        );
    }
    from_split.sort();
    assert!(!from_whole.is_empty(), "the fixture must produce findings");
    assert_eq!(
        from_whole, from_split,
        "the same routes are analysed differently once split across includes"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Lifecycle notifications out of order, malformed params, and random
/// content — the things an editor and a user actually do.
#[test]
fn the_server_survives_a_client_behaving_badly() {
    let (base, _root_uri, frag_uri) = setup("badclient");
    let (mut child, rx, mut stdin) = start(&base, "");
    // close before open, open twice, change a document never opened
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
            "textDocument":{"uri": frag_uri}}}),
    );
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri": format!("file://{}", base.join("never.cfg").display()),
                            "version": 2},
            "contentChanges":[{"text":"route[N] { exit; }\n"}]}}),
    );
    // malformed params, and a method that does not exist
    let mut id = 60;
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"uri": serde_json::Value::Null}),
        serde_json::json!({"uri": 42}),
        serde_json::json!({"uri": ["a"]}),
        serde_json::json!({"nope": "x"}),
    ] {
        id += 1;
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
                "params": bad}),
        );
        let want = id;
        let v = wait_for(
            &rx,
            |v| v["id"] == want,
            "analysisRoot with malformed params",
        );
        assert!(v.get("result").is_some() || v.get("error").is_some(), "{v}");
    }
    id += 1;
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/notARealMethod",
            "params":{}}),
    );
    let want = id;
    let v = wait_for(&rx, |v| v["id"] == want, "an unknown method");
    assert!(
        v.get("error").is_some(),
        "an unknown method is refused: {v}"
    );

    // random content in the buffer, then back to something valid
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for round in 0..40 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let n = (seed % 200) as usize;
        let junk: String = (0..n)
            .map(|k| {
                let c = ((seed >> (k % 48)) % 0x2f00 + 1) as u32;
                char::from_u32(c).unwrap_or('?')
            })
            .collect();
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri": frag_uri, "version": round + 10},
                "contentChanges":[{"text": junk}]}}),
        );
    }
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri": frag_uri, "version": 999},
            "contentChanges":[{"text": FRAG_TEXT}]}}),
    );
    // it must still be answering, and answering correctly
    id += 1;
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let want = id;
    let v = wait_for(&rx, |v| v["id"] == want, "analysisRoot after the abuse");
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "still correct after all of that: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// The same question, asked repeatedly, must give the same answer.
///
/// The graph is cached and invalidated from four places; a cache that
/// is rebuilt subtly differently, or invalidated by a read, shows up
/// as an answer that drifts rather than as an error.
#[test]
fn repeated_questions_do_not_drift() {
    let (base, root_uri, frag_uri) = setup("stable");
    let (mut child, rx, mut stdin) = start(&base, "");
    let mut answers: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut id = 80;
    for _ in 0..20 {
        id += 1;
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
                "params":{"uri": frag_uri}}),
        );
        let want = id;
        let v = wait_for(&rx, |v| v["id"] == want, "analysisRoot");
        answers.insert(v["result"].to_string());
    }
    assert_eq!(
        answers.len(),
        1,
        "the answer drifted across 20 asks: {answers:?}"
    );
    assert!(answers.iter().next().unwrap().contains("opensips.cfg"));

    // opening and closing documents must not move it either
    for _ in 0..5 {
        did_open(&mut stdin, &frag_uri, FRAG_TEXT);
        did_open(&mut stdin, &root_uri, ROOT_TEXT);
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                "textDocument":{"uri": root_uri}}}),
        );
    }
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
        "analysisRoot after open/close churn",
    );
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "open/close churn moved the answer: {v}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A folder added to the workspace after the server started.
///
/// The include graph is built from the client's workspace folders,
/// and those were read once, at `initialize`, and never again.  "Add
/// Folder to Workspace" is an ordinary thing to do — and every
/// fragment in the folder you just added stayed unrecognised until
/// the window was reloaded, with nothing to suggest why.
#[test]
fn a_workspace_folder_added_later_is_picked_up() {
    let (base, _root_uri, frag_uri) = setup("addfolder");
    // start with NO folder at all
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_BIN", "")
            .env(
                "OPENSIPS_LSP_CACHE_DIR",
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
            "workspaceFolders": []}}),
    );
    let init = wait_for(&rx, |v| v["id"] == 1, "init");
    // a server that does not SAY it handles folder changes will not be
    // told about them, so the capability is half the fix
    assert_eq!(
        init["result"]["capabilities"]["workspace"]["workspaceFolders"]["supported"], true,
        "the server must advertise that it handles workspace folders: {init}"
    );
    assert_eq!(
        init["result"]["capabilities"]["workspace"]["workspaceFolders"]["changeNotifications"],
        true,
        "and that it wants to be told when they change: {init}"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot with no folder");
    assert!(v["result"].is_null(), "nothing is open yet: {v}");

    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWorkspaceFolders",
            "params":{"event":{
                "added":[{"uri": format!("file://{}", base.display()), "name":"w"}],
                "removed":[]}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(
        &rx,
        |v| v["id"] == 3,
        "analysisRoot after the folder was added",
    );
    assert!(
        v["result"]
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "the folder was added; its fragments must be recognised: {v}"
    );

    // and removing it again takes them back out of scope
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWorkspaceFolders",
            "params":{"event":{"added":[],
                "removed":[{"uri": format!("file://{}", base.display()), "name":"w"}]}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"opensips/analysisRoot",
            "params":{"uri": frag_uri}}),
    );
    let v = wait_for(
        &rx,
        |v| v["id"] == 4,
        "analysisRoot after the folder was removed",
    );
    assert!(v["result"].is_null(), "the folder is gone again: {v}");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Edits, not shapes.
///
/// Everything so far asked whether a request ANSWERS from inside a
/// fragment.  These ask whether the answer is right: that a rename
/// produces edits which, applied, actually rename the thing in both
/// files; that a reference count counts the call site in the other
/// file; and that a generated stub lands in the buffer being edited
/// rather than in its parent.
#[test]
fn the_edits_a_fragment_produces_are_correct_not_merely_present() {
    let (base, root_uri, frag_uri) = setup("edits");
    let (mut child, rx, mut stdin) = start(&base, "");
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    // Take the publish BEFORE anything else asks a question:
    // `wait_for` discards what it is not waiting for, so a request
    // sent first eats the notification the quick fix needs.
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "diagnostics to drive the quick fix",
    );
    let published = v["params"]["diagnostics"].clone();
    assert!(
        published
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["message"].as_str().is_some_and(|m| m.contains("ghost"))),
        "the fixture must produce the finding the fix is for: {published}"
    );
    did_open(&mut stdin, &root_uri, ROOT_TEXT);

    // rename helper from its CALL SITE in the fragment
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/rename","params":{
            "textDocument":{"uri":frag_uri},"position":{"line":1,"character":11},
            "newName":"RENAMED"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "rename");
    let changes = v["result"]["changes"].as_object().expect("edits").clone();

    /// Apply edits to text, last first so earlier offsets stay valid.
    fn apply(text: &str, edits: &serde_json::Value) -> String {
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let mut es: Vec<&serde_json::Value> = edits.as_array().unwrap().iter().collect();
        es.sort_by_key(|e| {
            (
                std::cmp::Reverse(e["range"]["start"]["line"].as_u64().unwrap()),
                std::cmp::Reverse(e["range"]["start"]["character"].as_u64().unwrap()),
            )
        });
        for e in es {
            let l = e["range"]["start"]["line"].as_u64().unwrap() as usize;
            let s = e["range"]["start"]["character"].as_u64().unwrap() as usize;
            let t = e["range"]["end"]["character"].as_u64().unwrap() as usize;
            let line = &lines[l];
            lines[l] = format!(
                "{}{}{}",
                &line[..s],
                e["newText"].as_str().unwrap(),
                &line[t..]
            );
        }
        lines.join("\n")
    }

    let new_root = apply(ROOT_TEXT, &changes[&root_uri]);
    let new_frag = apply(FRAG_TEXT, &changes[&frag_uri]);
    assert!(
        new_root.contains("route[RENAMED]") && !new_root.contains("route[helper]"),
        "the DEFINITION lives in the root and must be rewritten there:\n{new_root}"
    );
    assert!(
        new_frag.contains("route(RENAMED)") && !new_frag.contains("route(helper)"),
        "the call site in the fragment must be rewritten too:\n{new_frag}"
    );

    // the fragment's own route is called once, from the root
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/codeLens",
            "params":{"textDocument":{"uri":frag_uri}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "codeLens");
    let titles: Vec<String> = v["result"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|l| l["command"]["title"].as_str().map(str::to_string))
        .collect();
    assert!(
        titles.iter().any(|t| t.starts_with('1')),
        "entry is called once, from the root; the count must span the \
         closure: {titles:?}"
    );

    // the quick fix for the undefined route goes into THIS file.
    // A real client passes the diagnostics it is showing, and the
    // action is offered against them — an empty context asks for
    // nothing and gets nothing.
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":4,"method":"textDocument/codeAction","params":{
            "textDocument":{"uri":frag_uri},
            "range":{"start":{"line":2,"character":10},"end":{"line":2,"character":14}},
            "context":{"diagnostics": published}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 4, "codeAction");
    let actions = v["result"].as_array().cloned().unwrap_or_default();
    assert!(!actions.is_empty(), "a stub must be offered for ghost: {v}");
    for a in &actions {
        if let Some(ch) = a["edit"]["changes"].as_object() {
            for uri in ch.keys() {
                assert_eq!(
                    uri, &frag_uri,
                    "a stub belongs in the file being edited, not in its parent: {a}"
                );
            }
        }
    }
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Every kind of watched-file event, not just "changed".
///
/// A root can appear, change and vanish, and the graph is built from
/// what is on disk.  Only one of those three was ever exercised.
#[test]
fn a_root_appearing_and_vanishing_is_noticed() {
    let (base, root_uri, frag_uri) = setup("events");
    let root_path = base.join("opensips.cfg");
    let saved = std::fs::read_to_string(&root_path).unwrap();
    std::fs::remove_file(&root_path).unwrap();

    let (mut child, rx, mut stdin) = start(&base, "");
    let mut id = 10;
    let ask = |stdin: &mut std::process::ChildStdin, id: &mut i32| {
        *id += 1;
        write_msg(
            stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":*id,"method":"opensips/analysisRoot",
                "params":{"uri": frag_uri}}),
        );
        let want = *id;
        wait_for(&rx, |v| v["id"] == want, "analysisRoot")["result"].clone()
    };
    assert!(ask(&mut stdin, &mut id).is_null(), "no root on disk yet");

    // CREATED
    std::fs::write(&root_path, &saved).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles",
            "params":{"changes":[{"uri": root_uri, "type": 1}]}}),
    );
    assert!(
        ask(&mut stdin, &mut id)
            .as_str()
            .is_some_and(|s| s.ends_with("opensips.cfg")),
        "a root created on disk must be picked up"
    );

    // CHANGED, to stop including the fragment
    std::fs::write(
        &root_path,
        saved.replace("include_file \"inc/routes.cfg\"\n", ""),
    )
    .unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles",
            "params":{"changes":[{"uri": root_uri, "type": 2}]}}),
    );
    assert!(
        ask(&mut stdin, &mut id).is_null(),
        "the root no longer includes it"
    );

    // CHANGED back, then DELETED
    std::fs::write(&root_path, &saved).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles",
            "params":{"changes":[{"uri": root_uri, "type": 2}]}}),
    );
    assert!(
        ask(&mut stdin, &mut id).as_str().is_some(),
        "and back again"
    );
    std::fs::remove_file(&root_path).unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles",
            "params":{"changes":[{"uri": root_uri, "type": 3}]}}),
    );
    assert!(ask(&mut stdin, &mut id).is_null(), "a deleted root is gone");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Churn: open, edit and close many documents, then check the server
/// still answers the same thing it did at the start.
///
/// Per-document state is kept in several maps, evicted from several
/// places.  A leak shows up as a slow drift rather than an error, and
/// a wrong eviction shows up as an answer that was right once.
#[test]
fn state_survives_repeated_open_edit_close_cycles() {
    let (base, root_uri, frag_uri) = setup("churn");
    let (mut child, rx, mut stdin) = start(&base, "");
    let mut id = 300;
    let ask = |stdin: &mut std::process::ChildStdin, id: &mut i32| -> String {
        *id += 1;
        write_msg(
            stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":*id,"method":"opensips/analysisRoot",
                "params":{"uri": frag_uri}}),
        );
        let want = *id;
        wait_for(&rx, |v| v["id"] == want, "analysisRoot")["result"].to_string()
    };
    let before = ask(&mut stdin, &mut id);
    for round in 0..25 {
        did_open(&mut stdin, &frag_uri, FRAG_TEXT);
        did_open(&mut stdin, &root_uri, ROOT_TEXT);
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
                "textDocument":{"uri": frag_uri, "version": round + 2},
                "contentChanges":[{"text": format!("{FRAG_TEXT}// {round}\n")}]}}),
        );
        for u in [&frag_uri, &root_uri] {
            write_msg(
                &mut stdin,
                &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{
                    "textDocument":{"uri": u}}}),
            );
        }
    }
    let after = ask(&mut stdin, &mut id);
    assert_eq!(before, after, "25 open/edit/close cycles moved the answer");
    // and it still analyses, rather than merely answering
    did_open(&mut stdin, &frag_uri, FRAG_TEXT);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == frag_uri,
        "diagnostics after the churn",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert!(msgs.iter().any(|m| m.contains("ghost")), "{msgs:?}");
    assert!(!msgs.iter().any(|m| m.contains("helper")), "{msgs:?}");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// A conditionally-guarded include, end to end — and OpenSIPS is not
/// Kamailio here.
///
/// OpenSIPS 4.0.1 reads an `include_file` inside an unmet `#!ifdef`
/// anyway (pinned by the proof suite against the real binary), so the
/// file IS part of that configuration and must be claimed either way.
/// The sibling server skips them, because Kamailio genuinely does not
/// open them.  The rule was derived from one engine and would have
/// been copied to the other unchecked.
#[test]
fn a_conditionally_guarded_include_is_still_claimed() {
    for (defined, label) in [(true, "defined"), (false, "not-defined")] {
        let base = std::env::temp_dir().join(format!("frag-cond-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).unwrap();
        let def = if defined { "#!define FEATURE_ON\n" } else { "" };
        std::fs::write(
            base.join("opensips.cfg"),
            format!(
                "{def}#!ifdef FEATURE_ON\ninclude_file \"inc/on.cfg\"\n#!endif\nroute[ROOT_ROUTE] {{\n    exit;\n}}\nroute {{\n    exit;\n}}\n"
            ),
        )
        .unwrap();
        std::fs::write(base.join("inc/on.cfg"), "route[COND] {\n    exit;\n}\n").unwrap();
        let frag_uri = format!("file://{}", base.join("inc/on.cfg").display());
        let (mut child, rx, mut stdin) = start(&base, "");
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"opensips/analysisRoot",
                "params":{"uri": frag_uri}}),
        );
        let v = wait_for(&rx, |v| v["id"] == 2, "analysisRoot");
        assert!(
            v["result"]
                .as_str()
                .is_some_and(|s| s.ends_with("opensips.cfg")),
            "OpenSIPS reads this include whether or not the symbol is {label}: {v}"
        );
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&base);
    }
}

/// A configuration split the way a real one is split.
///
/// Modelled on a 2400-line production config carved into 60 fragments
/// at its route boundaries: every fragment calls routes that live in
/// other fragments, which is what a split configuration IS.  Analysed
/// without knowing their root, those cross-references come back as
/// "not defined" — seventy-five of them on the config this test is
/// modelled on, on a configuration with nothing wrong with it.
///
/// The count is the point.  One fragment calling one parent route is
/// a mechanism test; this is the blast radius, and it is what makes
/// the difference between a usable editor and one nobody trusts.
#[test]
fn a_configuration_split_into_many_cross_calling_fragments_is_clean() {
    const N: usize = 40;
    let base = std::env::temp_dir().join(format!("frag-scale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("inc")).unwrap();

    // each fragment defines one route and calls its two neighbours,
    // so every one of them depends on files it does not include
    let piece = |i: usize| {
        format!(
            "route[R{i}] {{\n    route(R{});\n    route(R{});\n}}\n",
            (i + 1) % N,
            (i + N - 1) % N
        )
    };
    let mut root = String::from("");
    for i in 0..N {
        root.push_str(&format!("include_file \"inc/p{i}.cfg\"\n"));
        std::fs::write(base.join(format!("inc/p{i}.cfg")), piece(i)).unwrap();
    }
    root.push_str("route {\n    route(R0);\n}\n");
    std::fs::write(base.join("opensips.cfg"), &root).unwrap();

    let (mut child, rx, mut stdin) = start(&base, "");
    for i in 0..N {
        let uri = format!("file://{}", base.join(format!("inc/p{i}.cfg")).display());
        did_open(&mut stdin, &uri, &piece(i));
    }
    // one fragment gains a call to a route that exists nowhere: the
    // publish it produces is the signal that analysis really ran, and
    // its neighbours' silence then means something
    let ghost_uri = format!("file://{}", base.join("inc/p0.cfg").display());
    let ghost_text = format!(
        "{}route[EXTRA] {{\n    route(NO_SUCH_ROUTE);\n}}\n",
        piece(0)
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri": ghost_uri, "version": 2},
            "contentChanges":[{"text": ghost_text}]}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == ghost_uri
                && !v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "the planted finding",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert!(
        msgs.iter().any(|m| m.contains("NO_SUCH_ROUTE")),
        "a route defined nowhere is still reported: {msgs:?}"
    );
    assert!(
        !msgs.iter().any(|m| m.contains("route 'R")),
        "and the neighbours it calls are NOT: {msgs:?}"
    );

    // every fragment must know its root, or the silence above is luck
    let mut id = 400;
    for i in 0..N {
        id += 1;
        let uri = format!("file://{}", base.join(format!("inc/p{i}.cfg")).display());
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"opensips/analysisRoot",
                "params":{"uri": uri}}),
        );
        let want = id;
        let v = wait_for(&rx, |v| v["id"] == want, "analysisRoot");
        assert!(
            v["result"]
                .as_str()
                .is_some_and(|s| s.ends_with("opensips.cfg")),
            "fragment {i} of {N} did not find its root: {v}"
        );
    }
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}

/// Every setting, with a FRAGMENT open.
///
/// The settings were all tested against a single-file config, and the
/// fragment path reaches the analyzer, the cap and the code lens by a
/// different route — `merge_and_publish` and the pull handler each
/// resolve the analysis root at their own call site.  A toggle that
/// works on a whole config and not on an include is exactly the kind
/// of gap this feature keeps producing.
#[test]
fn every_setting_behaves_the_same_for_a_fragment() {
    let mk = |tag: &str| {
        let base = std::env::temp_dir().join(format!("frag-set-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("inc")).unwrap();
        std::fs::write(
            base.join("opensips.cfg"),
            "include_file \"inc/f.cfg\"\nroute[helper] {\n    exit;\n}\nroute {\n    route(entry);\n}\n",
        )
        .unwrap();
        let frag = "route[entry] {\n    route(helper);\n    route(G1);\n    route(G2);\n    route(G3);\n}\n";
        std::fs::write(base.join("inc/f.cfg"), frag).unwrap();
        let uri = format!("file://{}", base.join("inc/f.cfg").display());
        (base, uri, frag)
    };

    let boot = |base: &std::path::Path, opts: serde_json::Value| {
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
        let mut init = serde_json::json!({"opensipsPath": ""});
        for (k, v) in opts.as_object().unwrap() {
            init[k] = v.clone();
        }
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "capabilities":{},
                "initializationOptions": init,
                "workspaceFolders":[{"uri": format!("file://{}", base.display()), "name":"w"}]}}),
        );
        wait_for(&rx, |v| v["id"] == 1, "init");
        write_msg(
            &mut stdin,
            &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        );
        (child, rx, stdin)
    };

    // the analyzer ON: three undefined routes reported, the parent's not
    let (base, uri, frag) = mk("on");
    let (mut child, rx, mut stdin) = boot(&base, serde_json::json!({"analyzerDiagnostics": true}));
    did_open(&mut stdin, &uri, frag);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri,
        "diagnostics",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert_eq!(
        msgs.iter()
            .filter(|m| m.contains("G1") || m.contains("G2") || m.contains("G3"))
            .count(),
        3,
        "{msgs:?}"
    );
    assert!(!msgs.iter().any(|m| m.contains("helper")), "{msgs:?}");
    let full = msgs.len();
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);

    // maxDiagnostics caps what a fragment publishes
    let (base, uri, frag) = mk("cap");
    let (mut child, rx, mut stdin) = boot(&base, serde_json::json!({"maxDiagnostics": 2}));
    did_open(&mut stdin, &uri, frag);
    let v = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri,
        "capped diagnostics",
    );
    let n = v["params"]["diagnostics"].as_array().unwrap().len();
    assert!(
        n <= 2 && n < full,
        "the cap applies to a fragment too: {n} of {full}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);

    // live reconfiguration, with the fragment open: off, then on again
    let (base, uri, frag) = mk("live");
    let (mut child, rx, mut stdin) = boot(&base, serde_json::json!({"analyzerDiagnostics": true}));
    did_open(&mut stdin, &uri, frag);
    wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri,
        "initial diagnostics",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration",
            "params":{"settings":{"opensipsLsp":{"analyzerDiagnostics": false}}}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == uri
                && v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "the fragment cleared when the analyzer went off",
    );
    assert!(v["params"]["diagnostics"].as_array().unwrap().is_empty());
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"workspace/didChangeConfiguration",
            "params":{"settings":{"opensipsLsp":{"analyzerDiagnostics": true}}}}),
    );
    let v = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && v["params"]["uri"] == uri
                && !v["params"]["diagnostics"].as_array().unwrap().is_empty()
        },
        "and came back when it went on",
    );
    let msgs: Vec<&str> = v["params"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert_eq!(msgs.len(), full, "the same findings return: {msgs:?}");
    assert!(
        !msgs.iter().any(|m| m.contains("helper")),
        "and still in the ROOT's context, not the fragment's own: {msgs:?}"
    );
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&base);
}
