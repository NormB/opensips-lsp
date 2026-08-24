//! The built-in core catalogue, over real LSP stdio, with NOTHING
//! configured.
//!
//! `builtin_core_test.rs` proves the vendored file has the language in
//! it; this proves the running server actually serves it to a client
//! that has set no source tree — the state every user is in on the day
//! they install the extension.  The library-level test passed happily
//! while that path was untested, so the gap is worth a test of its own.

mod common;
use common::*;
use std::process::{Command, Stdio};

fn boot(
    tag: &str,
    text: &str,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    String,
    String,
) {
    let dir = std::env::temp_dir().join(format!("oslsp-builtin-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, text).unwrap();
    let uri = format!("file://{}", cfg.display());

    // no tree, no binary: the out-of-the-box state
    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_SRC", "")
            .env("OPENSIPS_LSP_BIN", "")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        &dir,
    );
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{}}}),
    );
    // the response has to be read before `initialized` is sent: a
    // notification that arrives mid-handshake is dropped, and the
    // harvest never runs
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
        "harvest-ready",
    )["params"]["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,"text":text}}}),
    );
    (child, rx, stdin, uri, ready)
}

fn complete(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    line: u32,
    ch: u32,
) -> Vec<String> {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":line,"character":ch}}}),
    );
    let r = wait_for(rx, |v| v["id"] == id, "completion");
    let items = r["result"]
        .as_array()
        .cloned()
        .or_else(|| r["result"]["items"].as_array().cloned())
        .unwrap_or_default();
    items
        .iter()
        .map(|i| i["label"].as_str().unwrap_or("").to_string())
        .collect()
}

fn hover(
    stdin: &mut std::process::ChildStdin,
    rx: &std::sync::mpsc::Receiver<serde_json::Value>,
    uri: &str,
    id: i64,
    line: u32,
    ch: u32,
) -> String {
    write_msg(
        stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":id,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":line,"character":ch}}}),
    );
    let r = wait_for(rx, |v| v["id"] == id, "hover");
    r["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

/// The complaint that produced the feature: typing `log_` offered
/// nothing but control-flow keywords until a source tree was set.
#[test]
fn a_global_parameter_completes_with_no_source_tree_configured() {
    let (mut child, rx, mut stdin, uri, ready) = boot("params", "log_\n");
    // the line reads "core docs" alone or "core and module docs"
    // depending on what the harvest left empty; both must name core
    assert!(
        ready.contains("built in from") && ready.contains("core"),
        "the readiness line must say the core docs are built in: {ready}"
    );

    let labels = complete(&mut stdin, &rx, &uri, 2, 0, 4);
    assert!(
        labels.contains(&"log_level".to_string()),
        "log_level must complete out of the box; got {} items: {:?}",
        labels.len(),
        &labels[..labels.len().min(25)]
    );
    // and it is not just the keyword list with one entry bolted on
    assert!(
        labels.len() > 100,
        "the whole core language should be offered, not {} items",
        labels.len()
    );
    let _ = child.kill();
}

/// Core functions and pseudo-variables come from the same catalogue,
/// so each needs its own witness rather than one standing for all.
#[test]
fn core_functions_and_pseudo_variables_complete_too() {
    let (mut child, rx, mut stdin, uri, _) = boot("fns", "route {\n    \n}\n");
    let labels = complete(&mut stdin, &rx, &uri, 2, 1, 4);
    assert!(
        labels.iter().any(|l| l == "xlog"),
        "a core function must be offered: {:?}",
        &labels[..labels.len().min(25)]
    );
    let _ = child.kill();

    let (mut child, rx, mut stdin, uri, _) = boot("pvars", "route {\n    $\n}\n");
    let labels = complete(&mut stdin, &rx, &uri, 2, 1, 5);
    assert!(
        labels.iter().any(|l| l.starts_with("$ru") || l == "$ru"),
        "a pseudo-variable must be offered: {:?}",
        &labels[..labels.len().min(25)]
    );
    let _ = child.kill();
}

/// Built-in documentation is pinned to one version, and a user whose
/// build differs has to be able to tell.  Hover says so.
#[test]
fn hover_on_a_built_in_entry_names_the_version_and_the_escape_hatch() {
    let (mut child, rx, mut stdin, uri, _) = boot("hover", "log_level=2\n");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":0,"character":3}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "hover");
    let text = r["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert!(
        text.contains("Built-in documentation from OpenSIPS"),
        "hover must disclose the provenance: {text:?}"
    );
    assert!(
        text.contains("opensipsSrc"),
        "hover must name the setting that overrides it: {text:?}"
    );
    let _ = child.kill();
}

/// `is_method` is a `sipmsgops` function, and it was the case that
/// made the core-only fallback look incomplete: the language
/// completed, but nothing a real config actually calls did.
#[test]
fn a_module_function_completes_with_no_source_tree_configured() {
    let (mut child, rx, mut stdin, uri, ready) = boot(
        "modfn",
        "loadmodule \"sipmsgops.so\"\nroute {\n    is_\n}\n",
    );
    assert!(
        ready.contains("module docs built in from"),
        "the readiness line must say module docs are built in: {ready}"
    );
    let labels = complete(&mut stdin, &rx, &uri, 2, 2, 7);
    assert!(
        labels.contains(&"is_method".to_string()),
        "is_method must complete once sipmsgops is loaded; got {} items",
        labels.len()
    );
    let _ = child.kill();
}

/// The built-ins must not defeat the loaded-module rule.  Offering
/// every module's functions everywhere would be worse than offering
/// none: it invites calls the config cannot make.
#[test]
fn a_module_function_stays_hidden_until_its_module_is_loaded() {
    let (mut child, rx, mut stdin, uri, _) =
        boot("modgate", "loadmodule \"tm.so\"\nroute {\n    is_\n}\n");
    let labels = complete(&mut stdin, &rx, &uri, 2, 2, 7);
    assert!(
        !labels.contains(&"is_method".to_string()),
        "sipmsgops is not loaded, so is_method must not be offered"
    );
    // ...while the module that IS loaded contributes normally
    let labels = complete(&mut stdin, &rx, &uri, 3, 2, 7);
    assert!(
        !labels.is_empty(),
        "tm's own functions should still be there"
    );
    let _ = child.kill();
}

/// With no tree there were no module NAMES either, so the first thing
/// anyone types in a fresh config completed to nothing.
#[test]
fn module_names_complete_inside_loadmodule_with_nothing_configured() {
    let (mut child, rx, mut stdin, uri, _) = boot("modnames", "loadmodule \"");
    let labels = complete(&mut stdin, &rx, &uri, 2, 0, 12);
    assert!(
        labels.len() > 100,
        "the whole module set should be offered, not {} items",
        labels.len()
    );
    assert!(
        labels.iter().any(|l| l.starts_with("sipmsgops")),
        "sipmsgops must be offerable: {:?}",
        &labels[..labels.len().min(10)]
    );
    let _ = child.kill();
}

/// A module the config never loads must not shadow the language.
///
/// The colliding name is DERIVED from the two vendored catalogues, not
/// written down here.  In 4.0.1 it happens to be `log_level`, which is
/// both a core global and an `opentelemetry` modparam — but whether a
/// given name collides is a fact about the pinned version, and either
/// side may rename it.  A hardcoded pair would keep passing while
/// proving nothing, or fail in a way that looks like a bug in the
/// server rather than a change upstream.
#[test]
fn a_core_global_outranks_a_same_named_param_of_an_unloaded_module() {
    let (name, module) = colliding_param();

    let (mut child, rx, mut stdin, uri, _) = boot("shadow", &format!("{name}=1\n"));
    let text = hover(&mut stdin, &rx, &uri, 30, 0, 1);
    assert!(
        text.contains("core parameter"),
        "the core global {name} must win when {module} is not loaded: {text:?}"
    );
    assert!(
        !text.contains(&format!("modparam of `{module}`")),
        "unloaded module {module} shadowed the language: {text:?}"
    );
    let _ = child.kill();

    // ...and loading the module does not change what a GLOBAL means.
    // A `name = value` statement is a core parameter and a
    // `modparam("m", "name", ...)` is that module's — two different
    // things that share a name, and the config says which is which.
    // Precedence alone got this wrong: with the module loaded, the
    // global assignment hovered as the module's parameter.
    let cfg = format!("loadmodule \"{module}.so\"\n{name}=1\n");
    let (mut child, rx, mut stdin, uri, _) = boot("shadowglobal", &cfg);
    let text = hover(&mut stdin, &rx, &uri, 32, 1, 1);
    assert!(
        text.contains("core parameter"),
        "a global assignment is the core parameter even with {module} loaded: {text:?}"
    );
    let _ = child.kill();

    // ...and when the module IS loaded, its parameter is the answer:
    // that is the modparam the config is actually setting.
    let cfg = format!("loadmodule \"{module}.so\"\nmodparam(\"{module}\", \"{name}\", 1)\n");
    let col = cfg
        .lines()
        .nth(1)
        .unwrap()
        .rfind(&name)
        .expect("the parameter name is on the modparam line") as u32;
    let (mut child, rx, mut stdin, uri, _) = boot("shadowload", &cfg);
    let text = hover(&mut stdin, &rx, &uri, 31, 1, col + 1);
    assert!(
        text.contains(&format!("modparam of `{module}`")),
        "a loaded module's parameter must win: {text:?}"
    );
    let _ = child.kill();
}

/// A parameter name that is both a core global and some module's
/// modparam in the pinned catalogues.
fn colliding_param() -> (String, String) {
    use std::collections::HashSet;
    let core: HashSet<&str> = opensips_lsp::catalog::builtin_core()
        .core
        .params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    for m in &opensips_lsp::catalog::builtin_modules().modules {
        if let Some(p) = m.params.iter().find(|p| core.contains(p.name.as_str())) {
            return (p.name.clone(), m.name.clone());
        }
    }
    panic!(
        "no core/module parameter collision in the pinned catalogues.  This gate \
         proves a core global outranks an unloaded module's parameter of the same \
         name, and needs a colliding name to do it: if a harvest ever has none, \
         replace this with a synthetic catalogue rather than deleting the gate."
    )
}
