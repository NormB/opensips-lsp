use opensips_lsp::logic::{diag_matches_file, resolve_bin};

#[test]
fn absent_everywhere_defaults_to_path_lookup() {
    assert_eq!(resolve_bin(None, None), Some("opensips".to_string()));
}

#[test]
fn explicit_empty_disables_diagnostics() {
    assert_eq!(resolve_bin(Some(""), None), None);
    assert_eq!(resolve_bin(None, Some("".into())), None);
    // explicit empty option wins over a set env var
    assert_eq!(resolve_bin(Some(""), Some("/usr/bin/x".into())), None);
}

#[test]
fn option_beats_env() {
    assert_eq!(
        resolve_bin(Some("/a/opensips"), Some("/b/opensips".into())),
        Some("/a/opensips".to_string())
    );
    assert_eq!(
        resolve_bin(None, Some("/b/opensips".into())),
        Some("/b/opensips".to_string())
    );
}

#[test]
fn diag_file_matching_tolerates_symlinked_tmp() {
    // exact match
    assert!(diag_matches_file(
        "/x/test.cfg",
        std::path::Path::new("/x/test.cfg")
    ));
    // empty diag file = global fallback, always attaches
    assert!(diag_matches_file("", std::path::Path::new("/x/test.cfg")));
    // different basename never matches
    assert!(!diag_matches_file(
        "/x/other.cfg",
        std::path::Path::new("/x/test.cfg")
    ));
    // same basename, different dir spelling (symlink case) DOES match
    assert!(diag_matches_file(
        "/private/tmp/a/test.cfg",
        std::path::Path::new("/tmp/a/test.cfg")
    ));
}
