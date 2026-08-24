//! The vendored module catalogue.
//!
//! `is_method` is a `sipmsgops` function, not core, so shipping only
//! the core language still left every module call undocumented and
//! `loadmodule "` offering nothing at all.  This is the same bargain
//! the core catalogue strikes, one level up: useful before the user
//! configures anything, pinned to one version, and replaced wholesale
//! by a tree they configure.

mod common;
use opensips_lsp::catalog;

#[test]
fn the_vendored_catalogue_has_the_modules_a_config_actually_loads() {
    let b = catalog::builtin_modules();
    assert!(!b.version.is_empty(), "it must say which version it is");
    let names: Vec<&str> = b.modules.iter().map(|m| m.name.as_str()).collect();
    for want in ["tm", "sl", "sipmsgops", "rr", "uac"] {
        assert!(names.contains(&want), "{want} missing from the built-ins");
    }
    assert!(b.modules.len() > 100, "{}", b.modules.len());

    // the function that prompted this: it must be attributed to the
    // module that exports it, or the loaded-module gate cannot work
    let owner = b
        .modules
        .iter()
        .find(|m| m.functions.iter().any(|f| f.name == "is_method"))
        .map(|m| m.name.as_str());
    assert_eq!(owner, Some("sipmsgops"), "is_method belongs to sipmsgops");

    let params: usize = b.modules.iter().map(|m| m.params.len()).sum();
    let functions: usize = b.modules.iter().map(|m| m.functions.len()).sum();
    assert!(
        params > 500 && functions > 200,
        "{params} params, {functions} functions"
    );
}

/// Every entry says where it came from.  A module's exports move
/// between releases, so a user reading built-in documentation has to
/// be able to tell it apart from their own build's.
#[test]
fn every_built_in_module_entry_names_the_version_it_came_from() {
    let b = catalog::builtin_modules();
    for m in &b.modules {
        for it in m.functions.iter().chain(m.params.iter()) {
            assert!(
                it.doc.contains(&b.version) && it.doc.contains("opensipsSrc"),
                "{}::{} does not say it is built-in documentation: {:?}",
                m.name,
                it.name,
                it.doc
            );
        }
    }
}

/// The freshness gate: the vendored file must still equal a harvest of
/// the pinned tree.  Regenerate with
/// `cargo run --example gen_module_catalog -- <tree> <version>`.
#[test]
fn the_vendored_module_catalogue_matches_a_fresh_harvest_of_the_pinned_tree() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let fresh = catalog::harvest_tree(std::path::Path::new(&tree));
    let b = catalog::builtin_modules();

    let names =
        |v: &[catalog::ModuleDoc]| -> Vec<String> { v.iter().map(|m| m.name.clone()).collect() };
    assert_eq!(
        names(&b.modules),
        names(&fresh),
        "vendored modules differ from the pinned tree — regenerate"
    );
    for (got, want) in b.modules.iter().zip(fresh.iter()) {
        let f = |v: &[catalog::Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };
        assert_eq!(
            f(&got.functions),
            f(&want.functions),
            "{} functions",
            got.name
        );
        assert_eq!(f(&got.params), f(&want.params), "{} params", got.name);
    }
}
