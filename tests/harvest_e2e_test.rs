//! What the harvest tells the client while it runs.
//!
//! Two surfaces, neither of which had any coverage:
//!
//!   - `workDoneProgress` around the post-initialize harvest. This
//!     server does not consult the client's capability; it asks to
//!     create a token and gives the client 500ms to answer, so an
//!     editor that does not do progress simply never replies and the
//!     harvest proceeds. That bound is the whole robustness of the
//!     design and nothing exercised it.
//!   - a `window/showMessage` WARNING when a CONFIGURED tree yields
//!     no documentation, which is a misconfiguration rather than a
//!     quiet default.

mod common;
use common::Server;
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// A tree with one documented module, in the 4.x markdown shape.
fn mk_tree(root: &Path) {
    let readme = root.join("modules/mymod/README.md");
    std::fs::create_dir_all(readme.parent().unwrap()).unwrap();
    std::fs::write(
        readme,
        "# mymod\n\n## Admin Guide\n\n### Exported Functions\n\n#### first_fn()\n\nOne.\n",
    )
    .unwrap();
}

fn boot(
    tag: &str,
    src: Option<&Path>,
) -> (
    common::Scratch,
    Server,
    Receiver<serde_json::Value>,
    ChildStdin,
    PathBuf,
) {
    let base_scratch = common::Scratch::new(&format!("oslsp-harv-boot-{tag}"));
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(&base).unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_opensips-lsp"));
    cmd.env("OPENSIPS_LSP_BIN", "")
        .env("OPENSIPS_LSP_CACHE_DIR", base.join("cache"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    match src {
        Some(p) => {
            cmd.env("OPENSIPS_LSP_SRC", p.display().to_string());
        }
        None => {
            cmd.env("OPENSIPS_LSP_SRC", "");
        }
    }
    let mut child = Server::new(cmd.spawn().unwrap(), &base);
    let rx = common::spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{"window":{"workDoneProgress":true}}}}),
    );
    common::wait_for(&rx, |v| v["id"] == 1, "initialize");
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (base_scratch, child, rx, stdin, base)
}

fn is_ready(v: &serde_json::Value) -> bool {
    v["method"] == "window/logMessage"
        && v["params"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("ready (")
}

/// Begin and end, on the token the server names. There is no report
/// phase — asserting one would be describing the sibling, not this
/// server.
#[test]
fn harvest_reports_progress_when_the_client_answers_the_create() {
    let outer_scratch = common::Scratch::new("oslsp-harv-tree");
    let outer = outer_scratch.path().to_path_buf();
    let tree = outer.join("tree");
    mk_tree(&tree);

    let (_scratch, mut child, rx, mut stdin, _) = boot("prog", Some(&tree));

    assert!(
        tree.exists(),
        "booting must leave the configured tree in place: {}",
        tree.display()
    );
    let create = common::wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create request",
    );
    let token = create["params"]["token"].clone();
    assert_eq!(
        token, "opensips-lsp/harvest",
        "the server names its own token: {create}"
    );
    common::write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":create["id"],"result":null}),
    );

    let begin = common::wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "begin",
        "progress begin",
    );
    assert_eq!(begin["params"]["token"], token);
    let title = begin["params"]["value"]["title"].as_str().unwrap_or("");
    assert!(
        title.to_lowercase().contains("harvest"),
        "the title must say what is happening: {begin}"
    );
    // the tree being read is named, so a user with several configured
    // can tell which one is slow
    assert_eq!(
        begin["params"]["value"]["message"].as_str().unwrap_or(""),
        tree.display().to_string(),
        "{begin}"
    );

    let end = common::wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "end",
        "progress end",
    );
    assert_eq!(end["params"]["token"], token);
    common::wait_for(&rx, is_ready, "ready");

    child.kill().ok();
}

/// A client that never answers the create must not stall startup.
///
/// This is the property the 500ms bound exists for: an editor with no
/// progress support simply does not reply, and a server that waited
/// for that reply would never finish harvesting and never become
/// useful. Nothing else in the suite would notice.
#[test]
fn a_client_that_never_answers_the_create_still_gets_a_ready_server() {
    let outer_scratch = common::Scratch::new("oslsp-harv-mute");
    let outer = outer_scratch.path().to_path_buf();
    let tree = outer.join("tree");
    mk_tree(&tree);

    let (_scratch, mut child, rx, _stdin, _) = boot("mute", Some(&tree));

    assert!(
        tree.exists(),
        "booting must leave the configured tree in place: {}",
        tree.display()
    );
    // the create arrives and is deliberately left unanswered
    common::wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create request",
    );
    // the harvest must still finish and the server still announce itself
    let ready = common::wait_for(&rx, is_ready, "ready despite an unanswered create");
    let msg = ready["params"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("documented modules"),
        "the harvest must have run: {msg:?}"
    );

    child.kill().ok();
}

/// A tree the user explicitly configured that documents nothing is a
/// misconfiguration, and saying so is the only way they find out.
#[test]
fn a_configured_tree_documenting_nothing_warns_visibly() {
    let outer_scratch = common::Scratch::new("oslsp-harv-empty");
    let outer = outer_scratch.path().to_path_buf();
    let tree = outer.join("empty-tree");
    std::fs::create_dir_all(&tree).unwrap();

    let (_scratch, mut child, rx, _stdin, _) = boot("empty", Some(&tree));

    assert!(
        tree.exists(),
        "booting must leave the configured tree in place: {}",
        tree.display()
    );
    let warn = common::wait_for(
        &rx,
        |v| {
            v["method"] == "window/showMessage"
                && v["params"]["message"]
                    .as_str()
                    .unwrap_or("")
                    .contains("yields no module documentation")
        },
        "empty-tree warning",
    );
    assert_eq!(warn["params"]["type"], 2, "MessageType::WARNING: {warn}");
    let msg = warn["params"]["message"].as_str().unwrap();
    assert!(msg.contains(&tree.display().to_string()), "{msg:?}");
    assert!(
        msg.contains("opensipsSrc"),
        "it must say what to check: {msg:?}"
    );
    // the message reached the client as a sentence. It shipped with
    // thirty spaces in the middle of it, from a continuation joined
    // without stripping its indentation, and no test looked.
    assert!(
        !msg.contains("  "),
        "the warning has a run of spaces in it: {msg:?}"
    );

    child.kill().ok();
}

/// No configured tree is the default, not a misconfiguration: the
/// built-in catalogue is the point. Warning there would train the
/// user to ignore the warning that matters.
#[test]
fn an_unconfigured_tree_stays_silent() {
    let (_scratch, mut child, rx, _stdin, _) = boot("silent", None);
    let ready = common::wait_for(&rx, is_ready, "ready");
    assert!(
        ready["params"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("built in from"),
        "the built-in catalogue must be in use: {ready}"
    );
    // nothing queued behind it complains
    while let Ok(v) = rx.recv_timeout(Duration::from_millis(300)) {
        assert!(
            v["method"] != "window/showMessage",
            "an unconfigured server must not warn: {v}"
        );
    }
    child.kill().ok();
}
