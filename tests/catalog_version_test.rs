//! A modparam warning must say which catalogue it was judged against.
//!
//! What a module exports moves between releases. Told only that a
//! parameter is "not documented", a reader cannot tell a typo from a
//! parameter their own build has and this catalogue does not — which
//! is exactly the case for anyone running a patched or newer OpenSIPS.

use opensips_lsp::catalog::{CatalogOrigin, Item, ModuleDoc};
use opensips_lsp::logic::catalog_diagnostics;

fn one_module() -> Vec<ModuleDoc> {
    vec![ModuleDoc {
        name: "usrloc".to_string(),
        params: vec![Item {
            name: "user_column".to_string(),
            detail: "string".to_string(),
            doc: String::new(),
        }],
        functions: Vec::new(),
    }]
}

const CFG: &str = "modparam(\"usrloc\", \"sql_reconcile_interval\", 120)\n";

#[test]
fn a_built_in_catalogue_names_its_version() {
    let origin = CatalogOrigin::BuiltIn("4.0.1".to_string());
    let diags = catalog_diagnostics(&one_module(), &origin, CFG);
    assert_eq!(diags.len(), 1, "one unknown parameter, got {diags:?}");
    let msg = &diags[0].message;
    assert!(
        msg.contains("4.0.1"),
        "the message must name the version it judged against, got {msg:?}"
    );
    assert!(
        msg.contains("sql_reconcile_interval") && msg.contains("usrloc"),
        "the message must still name the parameter and module, got {msg:?}"
    );
}

/// A configured tree is exact for the user's build by construction,
/// so naming a version there would be a lie — it says the tree.
#[test]
fn a_configured_tree_names_the_tree_not_a_version() {
    let diags = catalog_diagnostics(&one_module(), &CatalogOrigin::ConfiguredTree, CFG);
    assert_eq!(diags.len(), 1);
    let msg = &diags[0].message;
    assert!(msg.contains("configured source tree"), "got {msg:?}");
    assert!(
        !msg.contains("built in"),
        "a configured tree is not the built-in catalogue, got {msg:?}"
    );
}

/// The two origins must not produce the same sentence, or naming the
/// origin has bought nothing.
#[test]
fn the_two_origins_read_differently() {
    let a = catalog_diagnostics(&one_module(), &CatalogOrigin::BuiltIn("4.0.1".into()), CFG);
    let b = catalog_diagnostics(&one_module(), &CatalogOrigin::ConfiguredTree, CFG);
    assert_ne!(a[0].message, b[0].message);
}

/// A parameter the catalogue knows produces no diagnostic at all,
/// whatever the origin — the version note must not leak onto clean
/// configurations.
#[test]
fn a_known_parameter_is_silent() {
    let cfg = "modparam(\"usrloc\", \"user_column\", \"username\")\n";
    for origin in [
        CatalogOrigin::BuiltIn("4.0.1".to_string()),
        CatalogOrigin::ConfiguredTree,
    ] {
        assert!(
            catalog_diagnostics(&one_module(), &origin, cfg).is_empty(),
            "a known parameter must not warn"
        );
    }
}

/// A parameter absent from the release in use but present in a
/// neighbouring one is almost never a typo. Saying so turns an
/// unexplained warning into a version mismatch, which is a different
/// thing for the reader to go and do about it.
///
/// The parameter is found from the vendored catalogue rather than
/// hard-coded, so upstream removing or restoring one cannot leave
/// this asserting something that stopped being true.
#[test]
fn a_parameter_from_another_release_says_which() {
    let versioned = opensips_lsp::catalog::builtin_versioned();
    let newest = versioned.newest();
    let current = versioned.at(newest).expect("newest must resolve");

    // something an older release exported and the newest does not
    let mut found = None;
    'outer: for older in versioned.versions() {
        if older == newest {
            continue;
        }
        for m in versioned.at(older).expect("supported release") {
            // the MODULE must still exist in the current release: an
            // unknown module is silent by design, since the catalogue
            // may simply not cover it, so a parameter of a module
            // that was renamed away proves nothing about this clause
            let Some(now) = current.iter().find(|c| c.name == m.name) else {
                continue;
            };
            for p in &m.params {
                if !now.params.iter().any(|q| q.name == p.name) {
                    found = Some((m.name.clone(), p.name.clone()));
                    break 'outer;
                }
            }
        }
    }
    let (module, param) = found.expect("some parameter must have been dropped between releases");

    let origin = CatalogOrigin::BuiltIn(newest.to_string());
    let cfg = format!("modparam(\"{module}\", \"{param}\", 1)\n");
    let diags = catalog_diagnostics(&current, &origin, &cfg);
    assert_eq!(diags.len(), 1, "{module}::{param}: got {diags:?}");
    let msg = &diags[0].message;

    assert!(
        msg.contains("it exists in"),
        "{module}::{param} exists in an older release and the message must say so: {msg:?}"
    );
    // every release named must really have it, and the current one
    // must not be among them
    for v in versioned.versions() {
        let has = versioned
            .at(v)
            .expect("supported release")
            .iter()
            .any(|m| m.name == module && m.params.iter().any(|p| p.name == param));
        if v == newest {
            assert!(!has, "the current release must not export it");
            continue;
        }
        assert_eq!(
            msg.contains(v),
            has,
            "{module}::{param}: message names {v} as {} but the catalogue says {}",
            msg.contains(v),
            has
        );
    }
}

/// A name no release exports gets no cross-version clause — that one
/// really is a typo, and inventing a version for it would mislead.
#[test]
fn a_parameter_from_no_release_says_nothing_extra() {
    let versioned = opensips_lsp::catalog::builtin_versioned();
    let newest = versioned.newest();
    let current = versioned.at(newest).expect("newest must resolve");
    let origin = CatalogOrigin::BuiltIn(newest.to_string());

    let diags = catalog_diagnostics(
        &current,
        &origin,
        "modparam(\"usrloc\", \"definitely_not_a_parameter\", 1)\n",
    );
    assert_eq!(diags.len(), 1);
    assert!(
        !diags[0].message.contains("it exists in"),
        "got {:?}",
        diags[0].message
    );
}

/// A configured tree is one release by construction; this server
/// knows no others to compare it against, so it must not pretend to.
#[test]
fn a_configured_tree_gets_no_cross_version_clause() {
    let versioned = opensips_lsp::catalog::builtin_versioned();
    let current = versioned.at(versioned.newest()).expect("newest resolves");
    let diags = catalog_diagnostics(
        &current,
        &CatalogOrigin::ConfiguredTree,
        "modparam(\"proto_bin\", \"bin_async_local_connect_timeout\", 1)\n",
    );
    for d in &diags {
        assert!(!d.message.contains("it exists in"), "got {:?}", d.message);
    }
}
