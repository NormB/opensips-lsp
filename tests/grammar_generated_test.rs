//! The generated parser is committed, and must match `grammar.js`.
//!
//! Every consumer of a tree-sitter grammar builds from
//! `src/parser.c` and none of them run the CLI: nvim-treesitter
//! fetches `files = { "src/parser.c" }`, Helix and Zed compile that
//! path directly.  While `src/` was gitignored, this grammar could
//! not be built by any of the editors the README offers it to.
//!
//! Committing generated output buys that back at the cost of a file
//! that can go stale, so this fails when it has: it regenerates into a
//! scratch copy and compares.

use std::process::Command;

fn grammar_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tree-sitter-opensips")
}

#[test]
fn the_committed_parser_is_present_and_looks_generated() {
    let src = grammar_dir().join("src");
    for f in ["parser.c", "grammar.json", "node-types.json"] {
        let p = src.join(f);
        assert!(
            p.is_file(),
            "{} is missing — consumers build from it",
            p.display()
        );
    }
    let parser = std::fs::read_to_string(src.join("parser.c")).unwrap();
    assert!(
        parser.contains("tree_sitter_opensips"),
        "src/parser.c does not define this language"
    );
    assert!(
        src.join("tree_sitter/parser.h").is_file(),
        "the vendored header consumers compile against is missing"
    );
}

/// Gated on the CLI being available; CI installs it for the grammar
/// job, so this runs there.
#[test]
fn the_committed_parser_matches_grammar_js() {
    let dir = grammar_dir();
    let probe = Command::new("npx")
        .args(["--yes", "tree-sitter-cli@0.26", "--version"])
        .current_dir(&dir)
        .output();
    let Ok(p) = probe else {
        panic!("npx is required to check the generated parser is current")
    };
    assert!(p.status.success(), "tree-sitter-cli could not be resolved");

    let scratch = std::env::temp_dir().join(format!("ts-opensips-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).unwrap();
    for f in ["grammar.js", "package.json", "tree-sitter.json"] {
        std::fs::copy(dir.join(f), scratch.join(f)).unwrap();
    }
    let out = Command::new("npx")
        .args(["--yes", "tree-sitter-cli@0.26", "generate"])
        .current_dir(&scratch)
        .output()
        .expect("tree-sitter generate");
    assert!(
        out.status.success(),
        "generate failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fresh = std::fs::read_to_string(scratch.join("src/parser.c")).unwrap();
    let committed = std::fs::read_to_string(dir.join("src/parser.c")).unwrap();
    let _ = std::fs::remove_dir_all(&scratch);
    assert_eq!(
        fresh.len(),
        committed.len(),
        "src/parser.c is stale: grammar.js has changed since it was \
         generated. Run `npx tree-sitter-cli@0.26 generate` in tree-sitter-opensips and commit the result"
    );
    assert!(
        fresh == committed,
        "src/parser.c differs from a fresh generate"
    );
}
