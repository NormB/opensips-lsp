//! The server's configuration: where the opensips binary comes from (an
//! option, the environment, or a PATH lookup) and how a diagnostic's file
//! is matched to an open document.

mod common;

use opensips_lsp::logic;
use std::path::Path;

#[test]
fn absent_everywhere_defaults_to_path_lookup() {
    assert_eq!(logic::resolve_bin(None, None), Some("opensips".to_string()));
}

#[test]
fn explicit_empty_disables_diagnostics() {
    assert_eq!(logic::resolve_bin(Some(""), None), None);
    assert_eq!(logic::resolve_bin(None, Some("".into())), None);
    // explicit empty option wins over a set env var
    assert_eq!(
        logic::resolve_bin(Some(""), Some("/usr/bin/x".into())),
        None
    );
}

#[test]
fn option_beats_env() {
    assert_eq!(
        logic::resolve_bin(Some("/a/opensips"), Some("/b/opensips".into())),
        Some("/a/opensips".to_string())
    );
    assert_eq!(
        logic::resolve_bin(None, Some("/b/opensips".into())),
        Some("/b/opensips".to_string())
    );
}

#[test]
fn diag_file_matching_tolerates_symlinked_tmp() {
    // exact match
    assert!(
        logic::diag_matches_file("/x/test.cfg", Path::new("/x/test.cfg")),
        "a diagnostic naming the same path as the document belongs to that document"
    );
    // empty diag file = global fallback, always attaches
    assert!(
        logic::diag_matches_file("", Path::new("/x/test.cfg")),
        "a diagnostic with no file is the global fallback and attaches to any document"
    );
    // different basename never matches
    assert!(
        !logic::diag_matches_file("/x/other.cfg", Path::new("/x/test.cfg")),
        "a diagnostic for another basename belongs to a different file"
    );
    // same basename, different dir spelling (symlink case) DOES match
    assert!(
        logic::diag_matches_file("/private/tmp/a/test.cfg", Path::new("/tmp/a/test.cfg")),
        "the same basename under a different spelling of a symlinked directory must still match"
    );
}

#[test]
fn timeout_resolution_option_env_default() {
    use opensips_lsp::logic;
    use std::time::Duration;
    // option (milliseconds) wins
    assert_eq!(
        logic::resolve_timeout(Some(2500), Some("9000".into())),
        Duration::from_millis(2500)
    );
    // env next
    assert_eq!(
        logic::resolve_timeout(None, Some("300".into())),
        Duration::from_millis(300)
    );
    // garbage env falls through to the default
    assert_eq!(
        logic::resolve_timeout(None, Some("not-a-number".into())),
        Duration::from_secs(10)
    );
    assert_eq!(logic::resolve_timeout(None, None), Duration::from_secs(10));
    // zero disables nothing: clamp to at least 1ms so timeout() still fires sanely
    assert_eq!(
        logic::resolve_timeout(Some(0), None),
        Duration::from_millis(1)
    );
}

#[test]
fn existing_same_basename_files_do_not_cross_attach() {
    // the include-file attack: /a/test.cfg's error must not attach to
    // /b/test.cfg just because the basenames collide — when both paths
    // exist, canonical identity decides
    let base_scratch = common::Scratch::new("oslsp-canon");
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("a")).unwrap();
    std::fs::create_dir_all(base.join("b")).unwrap();
    std::fs::write(base.join("a/test.cfg"), "x").unwrap();
    std::fs::write(base.join("b/test.cfg"), "x").unwrap();
    let a = base.join("a/test.cfg");
    let b = base.join("b/test.cfg");
    assert!(
        !logic::diag_matches_file(&a.display().to_string(), &b),
        "distinct existing files with equal basenames must not match"
    );
    assert!(
        logic::diag_matches_file(&a.display().to_string(), &a),
        "an existing file matches a diagnostic naming its own path: {a:?}"
    );
}

#[test]
fn symlinked_spellings_of_the_same_file_match_canonically() {
    let base_scratch = common::Scratch::new("oslsp-link");
    let base = base_scratch.path().to_path_buf();
    std::fs::create_dir_all(base.join("real")).unwrap();
    std::fs::write(base.join("real/t.cfg"), "x").unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    let via_link = base.join("link/t.cfg");
    let real = base.join("real/t.cfg");
    assert!(
        logic::diag_matches_file(&real.display().to_string(), &via_link),
        "the path through the symlinked directory is the same file after canonicalization: \
         diag {real:?}, document {via_link:?}"
    );
    assert!(
        logic::diag_matches_file(&via_link.display().to_string(), &real),
        "the path through the symlinked directory is the same file after canonicalization: \
         diag {via_link:?}, document {real:?}"
    );
}
