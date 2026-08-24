//! Include-awareness + analyzer diagnostics over real stdio LSP:
//! cross-file definition, include-aware completion, and fast
//! (no-save, debounced) analyzer diagnostics.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn setup(base: &std::path::Path) -> (String, String) {
    std::fs::create_dir_all(base).unwrap();
    std::fs::write(base.join("inc.cfg"), "route[helper] {\n    exit;\n}\n").unwrap();
    let main_text = "include_file \"inc.cfg\"\nroute {\n    route(helper);\n}\n";
    let main = base.join("main.cfg");
    std::fs::write(&main, main_text).unwrap();
    (format!("file://{}", main.display()), main_text.to_string())
}

fn start(
    base: &std::path::Path,
    init_opts: serde_json::Value,
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
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "capabilities":{}, "initializationOptions": init_opts}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "init");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin)
}

#[test]
fn cross_file_definition_and_route_completion() {
    let base = std::env::temp_dir().join(format!("oslsp-inc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (uri, text) = setup(&base);
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"opensipsPath":""}));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // definition of `helper` (line 2, inside the name) → inc.cfg
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/definition","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "definition");
    let target = v["result"]["uri"].as_str().expect("cross-file location");
    assert!(
        target.ends_with("inc.cfg"),
        "definition must be in the include: {v}"
    );
    assert_eq!(v["result"]["range"]["start"]["line"], 0);
    // route( completion offers the include's route
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/completion","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":10}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "completion");
    let labels: Vec<&str> = v["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"helper"),
        "include-defined route completes: {labels:?}"
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn analyzer_diags_appear_on_change_without_save() {
    let base = std::env::temp_dir().join(format!("oslsp-and-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (uri, text) = setup(&base);
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"opensipsPath":""}));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // break the ref WITHOUT saving: analyzer diag must arrive
    let broken = "include_file \"inc.cfg\"\nroute {\n    route(gone);\n}\n";
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":broken}]}}),
    );
    let diag = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        },
        "analyzer diagnostics",
    );
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0]["message"].as_str().unwrap().contains("gone"));
    assert_eq!(ds[0]["source"], "opensips-lsp");
    assert_eq!(ds[0]["severity"], 2, "warning");
    assert_eq!(ds[0]["range"]["start"]["line"], 2);
    assert_eq!(diag["params"]["version"], 2);
    // fix it again: diags must clear
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":3},
        "contentChanges":[{"text":text}]}}),
    );
    let clear = wait_for(
        &rx,
        |v| v["method"] == "textDocument/publishDiagnostics" && v["params"]["version"] == 3,
        "clear",
    );
    assert!(
        clear["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn analyzer_diags_can_be_disabled() {
    let base = std::env::temp_dir().join(format!("oslsp-anoff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (uri, _) = setup(&base);
    let (mut child, rx, mut stdin) = start(
        &base,
        serde_json::json!({"opensipsPath":"", "analyzerDiagnostics": false}),
    );
    let broken = "route {\n    route(gone);\n}\n";
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":broken}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"text":broken.replace("gone", "also_gone")}]}}),
    );
    // bounded negative check: a hover round-trip after the debounce
    // window; no non-empty publish may precede its response
    std::thread::sleep(std::time::Duration::from_millis(150));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":9,"method":"textDocument/hover","params":{
        "textDocument":{"uri":uri},"position":{"line":0,"character":1}}}),
    );
    loop {
        let v = wait_for(
            &rx,
            |v| v["id"] == 9 || v["method"] == "textDocument/publishDiagnostics",
            "hover or diags",
        );
        if v["id"] == 9 {
            break;
        }
        assert!(
            v["params"]["diagnostics"].as_array().unwrap().is_empty(),
            "no analyzer diagnostics when disabled: {v}"
        );
    }
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn references_and_rename_cross_include_files() {
    let base = std::env::temp_dir().join(format!("oslsp-xref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (uri, text) = setup(&base);
    let inc_path = base.join("inc.cfg");
    let (mut child, rx, mut stdin) = start(&base, serde_json::json!({"opensipsPath":""}));
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    // references from the call site must include the include's def
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":2,"method":"textDocument/references","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11},
        "context":{"includeDeclaration":true}}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 2, "references");
    let locs = v["result"].as_array().expect("locations");
    assert_eq!(locs.len(), 2, "1 ref in main + 1 def in include: {locs:?}");
    assert!(
        locs.iter()
            .any(|l| l["uri"].as_str().unwrap().ends_with("inc.cfg")),
        "the include's definition must be listed: {locs:?}"
    );
    // rename must edit BOTH files
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/rename","params":{
        "textDocument":{"uri":uri},"position":{"line":2,"character":11},"newName":"assist"}}),
    );
    let v = wait_for(&rx, |v| v["id"] == 3, "rename");
    let changes = v["result"]["changes"].as_object().expect("changes");
    assert_eq!(
        changes.len(),
        2,
        "edits in main.cfg AND inc.cfg: {changes:?}"
    );
    let inc_uri = format!("file://{}", inc_path.display());
    let inc_edits = changes
        .get(&inc_uri)
        .and_then(|e| e.as_array())
        .expect("include edits");
    assert_eq!(inc_edits.len(), 1);
    assert_eq!(inc_edits[0]["newText"], "assist");
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn include_file_errors_surface_on_the_root_document() {
    let base = std::env::temp_dir().join(format!("oslsp-fdiag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (uri, text) = setup(&base);
    // stub checker attributing the error to the INCLUDE's absolute
    // path, exactly like the real parser does
    let stub = base.join("stub.sh");
    let sub = base.join("inc.cfg");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\necho \"CRITICAL:core:yyerror: parse error in {}:2:7-11: syntax error\" >&2\nexit 255\n",
            sub.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(&stub).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(&stub, perm).unwrap();

    let (mut child, rx, mut stdin) = start(
        &base,
        serde_json::json!({"opensipsPath": stub.display().to_string()}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
        "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    let diag = wait_for(
        &rx,
        |v| {
            v["method"] == "textDocument/publishDiagnostics"
                && !v["params"]["diagnostics"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .is_empty()
        },
        "re-attributed diagnostic",
    );
    assert_eq!(diag["params"]["uri"], serde_json::json!(uri));
    let ds = diag["params"]["diagnostics"].as_array().unwrap();
    let m = ds[0]["message"].as_str().unwrap();
    assert!(m.contains("inc.cfg"), "names the include: {m}");
    assert!(m.contains("line 2"), "carries the include-local line: {m}");
    assert!(m.contains("syntax error"), "{m}");
    // anchored at the include_file directive (line 0)
    assert_eq!(ds[0]["range"]["start"]["line"], 0);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
