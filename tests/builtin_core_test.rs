//! The vendored core catalogue.
//!
//! Core parameters, functions and pseudo-variables are the language,
//! not a module — requiring a source checkout before `log_level`
//! completes makes the extension useless out of the box. The vendored
//! copy fills that in, and must not drift from the version it claims.

mod common;
use opensips_lsp::catalog;

#[test]
fn the_vendored_catalogue_has_the_core_language_in_it() {
    let b = catalog::builtin_core();
    assert!(!b.version.is_empty(), "it must say which version it is");
    let params: Vec<&str> = b.core.params.iter().map(|p| p.name.as_str()).collect();
    // names as OpenSIPS 4.x actually spells them: `children` became
    // `udp_workers` and `listen` became `socket` in 3.x
    for want in ["log_level", "udp_workers", "socket", "mpath", "debug_mode"] {
        assert!(
            params.contains(&want),
            "{want} missing from the built-in params"
        );
    }
    assert!(b.core.functions.len() > 20, "{}", b.core.functions.len());
    assert!(b.core.pvars.len() > 50, "{}", b.core.pvars.len());
}

/// Every entry says where it came from, so nobody mistakes the pinned
/// The catalogue carries no provenance note of its own.
///
/// It used to: the note was baked into every entry as the catalogue
/// loaded, so it could not be turned off and it named a release the
/// user might not be using. It is applied by the server now, only
/// when `versionInHints` asks for it, which is also what lets it name
/// the right release. A note baked in here would be shown regardless
/// of the setting and would defeat both.
#[test]
fn the_catalogue_carries_no_baked_in_provenance_note() {
    let b = catalog::builtin_core();
    let mut checked = 0usize;
    for it in b
        .core
        .params
        .iter()
        .chain(b.core.functions.iter())
        .chain(b.core.pvars.iter())
    {
        checked += 1;
        assert!(
            !it.doc.contains("Built-in"),
            "{} carries a baked-in note: {:?}",
            it.name,
            it.doc
        );
        assert!(
            !it.doc.contains("opensipsSrc"),
            "{} carries a baked-in escape hatch: {:?}",
            it.name,
            it.doc
        );
    }
    // POSITIVE CONTROL: an empty catalogue would satisfy every
    // absence above.
    assert!(checked > 100, "only {checked} entries examined");
}

/// And the note the server applies says what it should.
#[test]
fn the_provenance_note_names_its_catalogue_and_release() {
    let note = catalog::version_note("core", "9.9.9");
    assert!(note.contains("OpenSIPS 9.9.9"), "{note:?}");
    assert!(note.contains("core documentation"), "{note:?}");
    assert!(note.contains("opensipsSrc"), "{note:?}");
}

/// The freshness gate: the vendored file must still equal a harvest of
/// the pinned tree.  Regenerate with
/// `cargo run --example gen_core_catalog -- <tree> <version>`.
#[test]
fn the_vendored_catalogue_matches_a_fresh_harvest_of_the_pinned_tree() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let fresh = catalog::harvest_core(std::path::Path::new(&tree));
    let b = catalog::builtin_core();
    let names = |v: &[catalog::Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };
    assert_eq!(
        names(&b.core.params),
        names(&fresh.params),
        "vendored core params differ from the pinned tree — regenerate"
    );
    assert_eq!(
        names(&b.core.functions),
        names(&fresh.functions),
        "vendored core functions differ from the pinned tree — regenerate"
    );
    assert_eq!(
        names(&b.core.pvars),
        names(&fresh.pvars),
        "vendored core pvars differ from the pinned tree — regenerate"
    );
}
