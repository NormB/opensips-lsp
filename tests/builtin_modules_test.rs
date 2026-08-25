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
/// every pinned tree it claims to cover.
///
/// `versioned_catalog_test.rs` proves that base-plus-deltas is a
/// lossless encoding, building one from the trees at test time. That
/// says nothing about the file actually SHIPPED — which is what a
/// user gets, and what goes stale. This reads the vendored file and
/// holds it against every release it names.
///
/// Regenerate with
/// `cargo run --example gen_versioned_catalog -- $(echo "$OPENSIPS_LSP_TEST_TREES" | tr , " ") > src/modules_builtin.json`.
#[test]
fn the_vendored_module_catalogue_matches_a_fresh_harvest_of_every_pinned_tree() {
    let raw = common::required_env("OPENSIPS_LSP_TEST_TREES");
    let trees: Vec<(String, std::path::PathBuf)> = raw
        .split(',')
        .filter_map(|e| {
            let (v, p) = e.split_once('=')?;
            Some((v.to_string(), std::path::PathBuf::from(p)))
        })
        .collect();
    let vendored = catalog::builtin_versioned();

    // the file must claim exactly the releases that were provisioned,
    // or it is covering something nothing here checks
    let provisioned: Vec<&str> = trees.iter().map(|(v, _)| v.as_str()).collect();
    assert_eq!(
        vendored.versions(),
        provisioned,
        "the vendored catalogue covers different releases than the proof environment provides"
    );

    for (version, tree) in &trees {
        let mut fresh = catalog::harvest_tree(tree);
        // the vendored file is stored canonically; a harvest must be
        // put in the same order before equality means anything
        catalog::canonicalize(&mut fresh);
        let got = vendored
            .at(version)
            .unwrap_or_else(|| panic!("{version} must resolve from the vendored file"));

        let names = |v: &[catalog::ModuleDoc]| -> Vec<String> {
            v.iter().map(|m| m.name.clone()).collect()
        };
        assert_eq!(
            names(&got),
            names(&fresh),
            "{version}: vendored modules differ from the pinned tree — regenerate"
        );
        for (a, b) in got.iter().zip(fresh.iter()) {
            assert_eq!(
                a, b,
                "{version}: module '{}' differs from the pinned tree — regenerate",
                a.name
            );
        }
    }
}

/// Parameters documented AFTER a module's first fenced example.
///
/// The harvester read any line starting with `#` as a heading, so the
/// `# single rtproxy` comment inside rtpengine's first example closed
/// the Exported Parameters section and the other thirteen parameters
/// were never harvested.  A configuration setting one of them was
/// told, in a warning, that the parameter does not exist.
#[test]
fn parameters_documented_after_an_example_are_in_the_catalogue() {
    let b = catalog::builtin_modules();
    for (module, param) in [
        ("rtpengine", "db_url"),
        ("rtpengine", "ping_enabled"),
        ("cachedb_redis", "connect_timeout"),
        ("cachedb_redis", "query_timeout"),
        ("dispatcher", "partition"),
        ("mid_registrar", "mode"),
        ("registrar", "max_contacts"),
        ("acc", "db_url"),
        ("presence", "db_url"),
    ] {
        let m = b
            .modules
            .iter()
            .find(|m| m.name == module)
            .unwrap_or_else(|| panic!("{module} missing from the catalogue"));
        assert!(
            m.params.iter().any(|p| p.name == param),
            "{module} has no {param}: {:?}",
            m.params.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
    }
}

/// A catalogue name is what a config writes in `modparam`.
///
/// `#### db_url(str)` — no space before the type — was harvested with
/// the type inside the name, so the entry could never match the call
/// site it was supposed to document.
#[test]
fn no_catalogue_entry_carries_its_type_in_its_name() {
    let b = catalog::builtin_modules();
    for m in &b.modules {
        for p in &m.params {
            assert!(
                !p.name.contains('(') && !p.name.contains(' '),
                "{}::{} is not a name a modparam could write",
                m.name,
                p.name
            );
        }
    }
}
