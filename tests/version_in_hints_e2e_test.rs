//! Whether built-in documentation repeats the release it came from.
//!
//! GIVEN the release is already on the status bar the whole time a
//! config is open, and every warning that turns on the release names
//! it,
//! WHEN a user hovers a built-in entry,
//! THEN the release is NOT repeated a third time — unless they have
//! asked for it, because someone reading a hover in isolation, or
//! pasting one into a ticket, wants the provenance with it.
//!
//! The setting decides; the default is off.

mod common;
use common::*;
use std::process::{Command, Stdio};

/// Hover text over `log_level` in a one-line config, with the given
/// initialization options.
/// Hover over `log_level` in a one-line config.
fn hover_text(tag: &str, opts: serde_json::Value) -> String {
    hover_text_at(tag, opts, "log_level=2\n", 0, 3)
}

fn hover_text_at(tag: &str, opts: serde_json::Value, body: &str, line: u32, ch: u32) -> String {
    let dir = std::env::temp_dir().join(format!("oslsp-vhints-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("t.cfg");
    std::fs::write(&cfg, body).unwrap();
    let uri = format!("file://{}", cfg.display());

    let mut child = Server::new(
        Command::new(env!("CARGO_BIN_EXE_opensips-lsp"))
            .env("OPENSIPS_LSP_SRC", "")
            .env("OPENSIPS_LSP_BIN", "")
            .env("OPENSIPS_LSP_VERSION", "")
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
            "capabilities":{}, "initializationOptions": opts}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    wait_for(
        &rx,
        |v| {
            v["method"] == "window/logMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("ready (")
        },
        "harvest-ready",
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"opensips-cfg","version":1,
                            "text":body}}}),
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":line,"character":ch}}}),
    );
    let r = wait_for(&rx, |v| v["id"] == 3, "hover");
    let text = r["result"]["contents"]["value"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    text
}

#[test]
fn by_default_a_hover_does_not_repeat_the_release() {
    let text = hover_text("default", serde_json::json!({}));
    // POSITIVE CONTROL: an empty hover would satisfy any "does not
    // contain" assertion below.
    assert!(
        text.contains("log_level"),
        "the hover must actually document the entry: {text:?}"
    );
    assert!(
        !text.contains("Built-in documentation"),
        "off by default — the status bar already says it: {text:?}"
    );
}

#[test]
fn with_the_setting_on_a_hover_names_the_release_and_the_escape_hatch() {
    let text = hover_text("on", serde_json::json!({"versionInHints": true}));
    assert!(
        text.contains("log_level"),
        "the hover must still document the entry: {text:?}"
    );
    assert!(
        text.contains("Built-in core documentation from OpenSIPS 4.0.1"),
        "asked for, so it names the release the CORE docs came from: {text:?}"
    );
    assert!(
        text.contains("opensipsSrc"),
        "and the setting that overrides it: {text:?}"
    );
}

/// The release setting selects the MODULE catalogue. Core docs are one
/// vendored artefact at a single release and do not move with it, so
/// the note over a core entry must keep naming the release those docs
/// actually came from. Saying the chosen one would be a lie about
/// provenance — which is the only thing this note is for.
#[test]
fn a_core_entry_names_its_own_release_not_the_selected_one() {
    let text = hover_text(
        "selected",
        serde_json::json!({"versionInHints": true, "opensipsVersion": "3.6.8"}),
    );
    assert!(
        text.contains("Built-in core documentation from OpenSIPS 4.0.1"),
        "core docs are pinned; the note must say where they came from: {text:?}"
    );
    assert!(
        !text.contains("core documentation from OpenSIPS 3.6.8"),
        "the chosen release does not apply to core docs: {text:?}"
    );
}

/// The two catalogues are pinned differently, and the note must say
/// so entry by entry.
///
/// This is the distinction a test of mine got wrong: it assumed a
/// core entry would name the selected release. Module documentation
/// carries several releases and follows `opensipsVersion`; core
/// documentation is one vendored artefact and names its own release
/// whatever is selected. A single hover cannot show both, so this
/// asks for one of each in the same server.
#[test]
fn a_module_entry_follows_the_selected_release_while_core_does_not() {
    let body =
        "log_level=2\nloadmodule \"usrloc.so\"\nmodparam(\"usrloc\", \"user_column\", \"u\")\n";
    let opts = serde_json::json!({"versionInHints": true, "opensipsVersion": "3.6.8"});

    // the module parameter: follows the selection
    let module = hover_text_at("kinds-mod", opts.clone(), body, 2, 22);
    assert!(
        module.contains("module documentation from OpenSIPS 3.6.8"),
        "a module entry must name the release in use: {module:?}"
    );

    // the core parameter, same server settings: names its own
    let core = hover_text_at("kinds-core", opts, body, 0, 3);
    assert!(
        core.contains("core documentation from OpenSIPS 4.0.1"),
        "a core entry must name where the core docs came from: {core:?}"
    );

    // and they must genuinely differ, or this proves nothing
    assert_ne!(
        module.contains("3.6.8"),
        core.contains("3.6.8"),
        "the two catalogues are pinned differently and must read differently"
    );
}

/// Selecting a release must not silently change the core catalogue.
///
/// The setting reaches the module catalogue only. If it ever started
/// moving the core one too, every core hover would claim a provenance
/// no vendored file supports.
#[test]
fn selecting_a_release_leaves_the_core_catalogue_alone() {
    let newest = hover_text_at(
        "core-newest",
        serde_json::json!({"versionInHints": true}),
        "log_level=2\n",
        0,
        3,
    );
    let older = hover_text_at(
        "core-older",
        serde_json::json!({"versionInHints": true, "opensipsVersion": "3.5.9"}),
        "log_level=2\n",
        0,
        3,
    );
    assert!(
        newest.contains("core documentation from OpenSIPS"),
        "control: the note must be present at all: {newest:?}"
    );
    assert_eq!(
        newest, older,
        "core documentation does not move with the selected release"
    );
}
