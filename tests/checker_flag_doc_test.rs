//! Documentation-vocabulary gate: the checker flag the pages promise
//! must be the flag the server actually passes.
//!
//! `opensips` accepts both `-c` and `-C` and they are not the same
//! check: `-C` additionally validates the flags of exported functions,
//! which is why the server runs it.  These two servers are sibling
//! repositories whose prose gets copied between them — the lowercase
//! flag travelling this way would document a weaker check than the one
//! that actually runs, and in kamailio-lsp the reverse leak reached
//! `SECURITY.md` and the Zed setup guide while every behavioural test
//! stayed green.  That is the whole gap: the argv is gated (`cli_test`,
//! `e2e_test` both assert it), the prose describing the argv was not.
//!
//! The flag is read from the call site rather than restated here, so
//! changing the invocation retargets this gate instead of stranding it
//! on a rule the code no longer follows.

use std::path::Path;

/// The flag the diagnostics subprocess is given, taken from the
/// `Command` construction in `src/server.rs`.
fn checker_flag() -> char {
    let (_, after) = include_str!("../src/server.rs")
        .split_once("Command::new(&bin)")
        .expect("the diagnostics subprocess call site");
    let (_, rest) = after
        .split_once(".arg(\"-")
        .expect("the checker must be invoked with a flag");
    rest.chars().next().expect("a flag letter")
}

/// The binary this server drives, derived from the package name so a
/// rename cannot leave the gate looking for the old one.
fn binary() -> &'static str {
    env!("CARGO_PKG_NAME")
        .strip_suffix("-lsp")
        .expect("package name is <binary>-lsp")
}

fn other_case(c: char) -> char {
    if c.is_ascii_uppercase() {
        c.to_ascii_lowercase()
    } else {
        c.to_ascii_uppercase()
    }
}

/// Pages that describe CURRENT behaviour.  `client/CHANGELOG.md` is a
/// historical record and has to be able to name the defect it fixed;
/// it is the one deliberate exception.
fn documentation_pages(root: &Path) -> Vec<std::path::PathBuf> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z", "*.md"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty() && *p != "client/CHANGELOG.md")
        .map(|p| root.join(p))
        .collect()
}

/// The two ways a page names a flag: backticked on its own, or written
/// out after the binary name.
fn hits(text: &str, binary: &str, flag: char) -> Vec<String> {
    [format!("`-{flag}`"), format!("{binary} -{flag}")]
        .into_iter()
        .filter(|needle| text.contains(needle))
        .collect()
}

#[test]
fn the_gate_fires_on_a_planted_wrong_flag() {
    // a gate that cannot fail is decoration; prove both spellings are
    // caught and that the correct flag is left alone
    let (bin, real) = (binary(), checker_flag());
    let wrong = other_case(real);
    assert!(!hits(&format!("run `-{wrong}` to check it"), bin, wrong).is_empty());
    assert!(
        !hits(
            &format!("the server runs {bin} -{wrong} for you"),
            bin,
            wrong
        )
        .is_empty()
    );
    assert!(
        hits(
            &format!("the server runs {bin} -{real} for you"),
            bin,
            wrong
        )
        .is_empty()
    );
}

#[test]
fn the_flag_is_read_from_the_call_site() {
    // guards the extractor itself: a refactor that moves the argv out
    // of reach must fail loudly here rather than silently return the
    // wrong letter and disarm the gate below
    let real = checker_flag();
    assert!(real.is_ascii_alphabetic(), "extracted {real:?}");
    assert!(
        include_str!("../src/server.rs").contains(&format!(".arg(\"-{real}\")")),
        "the extracted flag is not the one passed"
    );
}

#[test]
fn no_page_documents_a_checker_flag_the_server_never_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (bin, real) = (binary(), checker_flag());
    let wrong = other_case(real);

    let pages = documentation_pages(root);
    assert!(pages.len() > 5, "suspiciously small documentation set");

    let mut found = Vec::new();
    for p in pages {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        for h in hits(&text, bin, wrong) {
            found.push(format!("{}: {h}", p.display()));
        }
    }
    assert!(
        found.is_empty(),
        "the server runs `{bin} -{real}`, but these pages document `-{wrong}`:\n  {}",
        found.join("\n  ")
    );
}
