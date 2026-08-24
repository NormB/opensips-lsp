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
/// version's documentation for their own build's.
#[test]
fn every_built_in_entry_names_the_version_it_came_from() {
    let b = catalog::builtin_core();
    for it in b
        .core
        .params
        .iter()
        .chain(b.core.functions.iter())
        .chain(b.core.pvars.iter())
    {
        assert!(
            it.doc.contains(&b.version) && it.doc.contains("opensipsSrc"),
            "{} does not say it is built-in documentation: {:?}",
            it.name,
            it.doc
        );
    }
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
