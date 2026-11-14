//! Clone gate: the git hooks must be turned on in the clone the tests
//! run in.
//!
//! `.githooks/` holds the commit and push gates, and git runs them only
//! when the clone says `core.hooksPath .githooks`. A clone without that
//! line commits with no check at all and says nothing: `git commit`
//! returns 0 either way. This test is the place where such a clone is
//! told, with the command that fixes it.
//!
//! CI is exempt: its checkout never has the setting, and its jobs run
//! the same gates themselves (.github/workflows/ci.yml). A source tree
//! that is not a git clone is exempt too: nothing is committed from it.

use std::process::Command;

/// The setting the hooks need, as CONTRIBUTING.md gives it.
const HOOKS_PATH: &str = ".githooks";

/// Whether a clone may commit: `Err` holds what to tell its owner.
///
/// `on_ci` is whether the tests run in GitHub Actions, `in_clone`
/// whether the source tree is a git work tree, and `hooks_path` the
/// clone's `core.hooksPath`, if it has one.
fn hooks_verdict(on_ci: bool, in_clone: bool, hooks_path: Option<&str>) -> Result<(), String> {
    if on_ci || !in_clone {
        return Ok(());
    }
    let set = hooks_path
        .map(|p| p.trim().trim_end_matches('/'))
        .unwrap_or_default();
    if set == HOOKS_PATH || set.ends_with(&format!("/{HOOKS_PATH}")) {
        return Ok(());
    }
    let found = if set.is_empty() {
        "not set".to_string()
    } else {
        format!("`{set}`")
    };
    Err(format!(
        "the git hooks are off in this clone (core.hooksPath is {found}), so commits here skip the \
         commit and push gates; turn them on with: git config core.hooksPath {HOOKS_PATH}"
    ))
}

/// What `git <args>` prints in the source tree, when it succeeds.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[test]
fn the_setting_from_the_contributor_page_is_accepted() {
    assert_eq!(
        hooks_verdict(false, true, Some(".githooks")),
        Ok(()),
        "the documented setting"
    );
}

#[test]
fn a_trailing_slash_or_newline_is_accepted() {
    assert_eq!(
        hooks_verdict(false, true, Some(".githooks/\n")),
        Ok(()),
        "as git config prints it"
    );
}

#[test]
fn an_absolute_path_to_the_hooks_directory_is_accepted() {
    assert_eq!(
        hooks_verdict(false, true, Some("/src/opensips-lsp/.githooks")),
        Ok(()),
        "absolute path"
    );
}

#[test]
fn a_clone_with_no_setting_is_refused_with_the_command() {
    let told = hooks_verdict(false, true, None).expect_err("no core.hooksPath must be refused");
    assert!(
        told.contains("core.hooksPath is not set"),
        "the message says what was found: {told}"
    );
    assert!(
        told.contains("git config core.hooksPath .githooks"),
        "the message gives the command: {told}"
    );
}

#[test]
fn an_empty_setting_is_refused() {
    assert!(
        hooks_verdict(false, true, Some("")).is_err(),
        "an empty core.hooksPath runs no hook"
    );
}

#[test]
fn another_hooks_directory_is_refused_and_named() {
    let told = hooks_verdict(false, true, Some(".git/hooks")).expect_err("another directory");
    assert!(
        told.contains("`.git/hooks`"),
        "the message names the directory found: {told}"
    );
}

#[test]
fn a_directory_that_only_ends_like_the_hooks_directory_is_refused() {
    assert!(
        hooks_verdict(false, true, Some("x.githooks")).is_err(),
        "not the hooks directory"
    );
}

#[test]
fn ci_is_exempt_because_its_jobs_run_the_gates() {
    assert_eq!(
        hooks_verdict(true, true, None),
        Ok(()),
        "a CI checkout has no core.hooksPath"
    );
}

#[test]
fn a_source_tree_that_is_not_a_clone_is_exempt() {
    assert_eq!(
        hooks_verdict(false, false, None),
        Ok(()),
        "nothing is committed from a tarball"
    );
}

/// The gate itself: this clone.
#[test]
fn this_clone_has_the_hooks_turned_on() {
    let on_ci = std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true");
    let in_clone = git(&["rev-parse", "--is-inside-work-tree"]).is_some_and(|v| v == "true");
    let hooks_path = git(&["config", "--get", "core.hooksPath"]);
    if let Err(told) = hooks_verdict(on_ci, in_clone, hooks_path.as_deref()) {
        panic!("{told}");
    }
}
