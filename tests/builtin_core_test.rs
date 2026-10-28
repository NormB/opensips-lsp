//! The vendored core catalogue.
//!
//! Core parameters, functions and pseudo-variables are the language,
//! not a module — requiring a source checkout before `log_level`
//! completes makes the extension useless out of the box. The vendored
//! copy fills that in, and must not drift from the version it claims.

mod common;
use opensips_lsp::catalog;
use opensips_lsp::catalog::Item;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

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
    let fresh = catalog::harvest_core(Path::new(&tree));
    let b = catalog::builtin_core();
    let names = |v: &[Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };
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

/// The vendored catalogue must match in its TEXT, not just its names.
///
/// The drift gate above compares names. The regression that dropped
/// every default and every worked example from every hover changed no
/// name at all, so that gate would have passed it without a murmur.
/// Names are the cheap half of a catalogue; the text is what a reader
/// is actually shown.
#[test]
fn the_vendored_catalogue_matches_a_fresh_harvest_in_its_text_too() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let fresh = catalog::harvest_core(Path::new(&tree));
    let b = catalog::builtin_core();

    let mut differ: Vec<String> = Vec::new();
    let mut compared = 0usize;
    for (what, shipped, harvested) in [
        ("params", &b.core.params, &fresh.params),
        ("functions", &b.core.functions, &fresh.functions),
        ("pvars", &b.core.pvars, &fresh.pvars),
    ] {
        compared += shipped
            .iter()
            .filter(|s| harvested.iter().any(|h| h.name == s.name))
            .count();
        differ.extend(text_differences(what, shipped, harvested));
    }
    // POSITIVE CONTROL: both sides were read.
    assert!(compared > 200, "only {compared} entries compared");
    assert!(
        differ.is_empty(),
        "vendored text differs from the pinned tree — regenerate. This is the \
         half the name comparison cannot see: {differ:?}"
    );
}

/// The entries whose text differs between the shipped and the
/// harvested list, as `what/name`, paired by name: an entry on one
/// side only is reported once, and does not shift the others.
fn text_differences(what: &str, shipped: &[Item], harvested: &[Item]) -> Vec<String> {
    let by_name = |items: &[Item]| -> BTreeMap<String, (String, String)> {
        items
            .iter()
            .map(|i| (i.name.clone(), (i.detail.clone(), i.doc.clone())))
            .collect()
    };
    let (s, h) = (by_name(shipped), by_name(harvested));
    let names: BTreeSet<&String> = s.keys().chain(h.keys()).collect();
    names
        .into_iter()
        .filter(|n| s.get(*n) != h.get(*n))
        .map(|n| format!("{what}/{n}"))
        .collect()
}

/// One release adding one entry must be reported as that one entry.
///
/// Pairing the two lists by position made every entry after an
/// insertion look changed: 4.0.2 added `REACTOR_DBG_FD_CHECK` and the
/// gate named 49 parameters, of which only that one had changed.
#[test]
fn an_inserted_entry_is_the_only_difference_reported() {
    let item = |name: &str, doc: &str| Item {
        name: name.to_string(),
        detail: "core parameter".to_string(),
        doc: doc.to_string(),
    };
    let shipped = vec![item("a", "A"), item("c", "C"), item("d", "D")];
    let harvested = vec![
        item("a", "A"),
        item("b", "B"),
        item("c", "C"),
        item("d", "D"),
    ];
    assert_eq!(
        text_differences("params", &shipped, &harvested),
        vec!["params/b"]
    );

    // and a real text change is still reported, by name
    let changed = vec![item("a", "A"), item("c", "changed"), item("d", "D")];
    assert_eq!(
        text_differences("params", &shipped, &changed),
        vec!["params/c"]
    );
}

/// And the gate must cover every field it ships.
///
/// It compared three. Routes, statements, the socket modifiers and
/// the log levels were vendored and held against nothing, so any of
/// them could go stale — or empty — with no test noticing.
#[test]
fn the_drift_gate_covers_every_field_of_the_catalogue() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let fresh = catalog::harvest_core(Path::new(&tree));
    let b = catalog::builtin_core();
    let names = |v: &[Item]| -> Vec<String> { v.iter().map(|i| i.name.clone()).collect() };

    for (what, shipped, harvested) in [
        ("routes", &b.core.routes, &fresh.routes),
        ("statements", &b.core.statements, &fresh.statements),
        (
            "socket_modifiers",
            &b.core.socket_modifiers,
            &fresh.socket_modifiers,
        ),
    ] {
        // POSITIVE CONTROL: a field that harvests empty would make
        // the comparison trivially true.
        assert!(!harvested.is_empty(), "{what} harvested empty");
        assert_eq!(
            names(shipped),
            names(harvested),
            "vendored {what} differ from the pinned tree — regenerate"
        );
    }
    assert!(!fresh.log_levels.is_empty(), "log_levels harvested empty");
    assert_eq!(
        b.core.log_levels, fresh.log_levels,
        "vendored log levels differ from the pinned tree — regenerate"
    );
}
