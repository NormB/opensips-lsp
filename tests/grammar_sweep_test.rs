//! The tree-sitter grammar must parse every configuration the real
//! parser accepts.
//!
//! Editors other than VS Code (Neovim, Helix, Zed) build their view of
//! the file from this grammar alone, so a construct it does not know
//! shows up there as an error in a valid config.  The corpus is every
//! `.cfg` in the pinned OpenSIPS tree that `opensips -C` does not
//! reject with `syntax error` — module-loading failures are not the
//! grammar's business — plus the fixtures in `tests/grammar/`, which
//! hold the constructs the shipped examples do not exercise and which
//! the binary must accept as written.

mod common;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[test]
fn the_grammar_parses_every_config_the_real_parser_accepts() {
    let tree = PathBuf::from(common::required_env("OPENSIPS_LSP_TEST_TREE"));
    let bin = common::required_env("OPENSIPS_LSP_TEST_BIN");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grammar");

    let mut shipped = Vec::new();
    common::cfgs_under(&tree, &mut shipped);
    shipped.sort();
    let mut fixture_files = Vec::new();
    common::cfgs_under(&fixtures, &mut fixture_files);
    fixture_files.sort();
    assert!(!fixture_files.is_empty(), "the fixtures went missing");

    for f in &fixture_files {
        assert!(
            !common::syntax_error(&bin, f),
            "{}: a fixture the real parser rejects proves nothing",
            f.display()
        );
    }
    let (accepted, rejected): (Vec<PathBuf>, Vec<PathBuf>) = shipped
        .into_iter()
        .partition(|f| !common::syntax_error(&bin, f));
    // POSITIVE CONTROL: a tree with nothing accepted makes this vacuous
    assert!(
        accepted.len() >= 20,
        "only {} shipped configs accepted ({} rejected)",
        accepted.len(),
        rejected.len()
    );

    let grammar = Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-opensips");
    let files = accepted.iter().chain(&fixture_files).map(|f| f.as_os_str());
    let args = [OsStr::new("parse"), OsStr::new("--quiet")]
        .into_iter()
        .chain(files);
    let out = common::tree_sitter(&grammar, None, args);
    let report = String::from_utf8_lossy(&out.stdout);
    let failed: Vec<&str> = report
        .lines()
        .filter(|l| l.contains("(ERROR") || l.contains("(MISSING"))
        .collect();
    assert!(
        failed.is_empty(),
        "{} of {} configs the real parser accepts do not parse:\n{}",
        failed.len(),
        accepted.len() + fixture_files.len(),
        failed.join("\n")
    );
    eprintln!(
        "grammar parses {} accepted configs ({} rejected by the real parser skipped)",
        accepted.len() + fixture_files.len(),
        rejected.len()
    );
}

/// `queries/folds.scm` is read by editors, never by `tree-sitter test`,
/// so nothing else would notice it naming a node the grammar does not
/// have — the query then fails to load and folding silently stops.
/// On the highlight fixture it must fold the route block, the `if`
/// block inside it and the nested block comment.
#[test]
fn the_fold_query_loads_and_folds_blocks_and_comments() {
    let grammar = Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-opensips");
    let out = common::tree_sitter(
        &grammar,
        None,
        ["query", "queries/folds.scm", "test/highlight/basics.cfg"],
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "folds.scm does not load: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout.matches("capture:").count(),
        3,
        "expected 3 folds:\n{stdout}"
    );
}
