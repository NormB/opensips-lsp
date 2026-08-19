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

#[test]
fn timeout_resolution_option_env_default() {
    use opensips_lsp::logic::resolve_timeout;
    use std::time::Duration;
    // option (milliseconds) wins
    assert_eq!(
        resolve_timeout(Some(2500), Some("9000".into())),
        Duration::from_millis(2500)
    );
    // env next
    assert_eq!(
        resolve_timeout(None, Some("300".into())),
        Duration::from_millis(300)
    );
    // garbage env falls through to the default
    assert_eq!(
        resolve_timeout(None, Some("not-a-number".into())),
        Duration::from_secs(10)
    );
    assert_eq!(resolve_timeout(None, None), Duration::from_secs(10));
    // zero disables nothing: clamp to at least 1ms so timeout() still fires sanely
    assert_eq!(resolve_timeout(Some(0), None), Duration::from_millis(1));
}

#[test]
fn existing_same_basename_files_do_not_cross_attach() {
    // the include-file attack: /a/test.cfg's error must not attach to
    // /b/test.cfg just because the basenames collide — when both paths
    // exist, canonical identity decides
    let base = std::env::temp_dir().join(format!("oslsp-canon-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("a")).unwrap();
    std::fs::create_dir_all(base.join("b")).unwrap();
    std::fs::write(base.join("a/test.cfg"), "x").unwrap();
    std::fs::write(base.join("b/test.cfg"), "x").unwrap();
    let a = base.join("a/test.cfg");
    let b = base.join("b/test.cfg");
    assert!(
        !diag_matches_file(&a.display().to_string(), &b),
        "distinct existing files with equal basenames must not match"
    );
    assert!(diag_matches_file(&a.display().to_string(), &a));
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn symlinked_spellings_of_the_same_file_match_canonically() {
    let base = std::env::temp_dir().join(format!("oslsp-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("real")).unwrap();
    std::fs::write(base.join("real/t.cfg"), "x").unwrap();
    std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
    let via_link = base.join("link/t.cfg");
    let real = base.join("real/t.cfg");
    assert!(diag_matches_file(&real.display().to_string(), &via_link));
    assert!(diag_matches_file(&via_link.display().to_string(), &real));
    let _ = std::fs::remove_dir_all(&base);
}
