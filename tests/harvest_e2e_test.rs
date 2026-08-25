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
use common::*;
use std::process::{Command, Stdio};

/// A tree with one documented module, in the 4.x markdown shape.
fn mk_tree(root: &std::path::Path) {
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
    src: Option<&std::path::Path>,
) -> (
    Server,
    std::sync::mpsc::Receiver<serde_json::Value>,
    std::process::ChildStdin,
    std::path::PathBuf,
) {
    let base = std::env::temp_dir().join(format!("oslsp-harv-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
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
    let rx = spawn_reader(&mut child);
    let mut stdin = child.stdin.take().unwrap();
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "capabilities":{"window":{"workDoneProgress":true}}}}),
    );
    wait_for(&rx, |v| v["id"] == 1, "initialize");
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    (child, rx, stdin, base)
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
    let outer = std::env::temp_dir().join(format!("oslsp-harv-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    let tree = outer.join("tree");
    mk_tree(&tree);

    let (mut child, rx, mut stdin, base) = boot("prog", Some(&tree));
    let create = wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create request",
    );
    let token = create["params"]["token"].clone();
    assert_eq!(
        token, "opensips-lsp/harvest",
        "the server names its own token: {create}"
    );
    write_msg(
        &mut stdin,
        &serde_json::json!({"jsonrpc":"2.0","id":create["id"],"result":null}),
    );

    let begin = wait_for(
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

    let end = wait_for(
        &rx,
        |v| v["method"] == "$/progress" && v["params"]["value"]["kind"] == "end",
        "progress end",
    );
    assert_eq!(end["params"]["token"], token);
    wait_for(&rx, is_ready, "ready");

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outer);
}

/// A client that never answers the create must not stall startup.
///
/// This is the property the 500ms bound exists for: an editor with no
/// progress support simply does not reply, and a server that waited
/// for that reply would never finish harvesting and never become
/// useful. Nothing else in the suite would notice.
#[test]
fn a_client_that_never_answers_the_create_still_gets_a_ready_server() {
    let outer = std::env::temp_dir().join(format!("oslsp-harv-mute-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    let tree = outer.join("tree");
    mk_tree(&tree);

    let (mut child, rx, _stdin, base) = boot("mute", Some(&tree));
    // the create arrives and is deliberately left unanswered
    wait_for(
        &rx,
        |v| v["method"] == "window/workDoneProgress/create",
        "progress create request",
    );
    // the harvest must still finish and the server still announce itself
    let ready = wait_for(&rx, is_ready, "ready despite an unanswered create");
    let msg = ready["params"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("documented modules"),
        "the harvest must have run: {msg:?}"
    );

    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outer);
}

/// A tree the user explicitly configured that documents nothing is a
/// misconfiguration, and saying so is the only way they find out.
#[test]
fn a_configured_tree_documenting_nothing_warns_visibly() {
    let outer = std::env::temp_dir().join(format!("oslsp-harv-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&outer);
    let tree = outer.join("empty-tree");
    std::fs::create_dir_all(&tree).unwrap();

    let (mut child, rx, _stdin, base) = boot("empty", Some(&tree));
    let warn = wait_for(
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
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outer);
}

/// No configured tree is the default, not a misconfiguration: the
/// built-in catalogue is the point. Warning there would train the
/// user to ignore the warning that matters.
#[test]
fn an_unconfigured_tree_stays_silent() {
    let (mut child, rx, _stdin, base) = boot("silent", None);
    let ready = wait_for(&rx, is_ready, "ready");
    assert!(
        ready["params"]["message"]
            .as_str()
            .unwrap_or("")
            .contains("built in from"),
        "the built-in catalogue must be in use: {ready}"
    );
    // nothing queued behind it complains
    while let Ok(v) = rx.recv_timeout(std::time::Duration::from_millis(300)) {
        assert!(
            v["method"] != "window/showMessage",
            "an unconfigured server must not warn: {v}"
        );
    }
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&base);
}
