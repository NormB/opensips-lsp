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
