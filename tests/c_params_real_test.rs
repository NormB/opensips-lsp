//! The C-table extraction against the real OpenSIPS tree.
//!
//! The unit tests in `c_params_test.rs` prove the parser against
//! fixtures. These prove it against the thing it actually reads, and
//! they carry the positive controls: a derived rule that silently
//! stops matching would otherwise excuse everything and report green.

mod common;

use opensips_lsp::catalog::{builtin_modules, param_names_from_c};
use std::path::Path;

fn tree() -> String {
    common::required_env("OPENSIPS_LSP_TEST_TREE")
}

fn module_dirs(root: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir(root.join("modules"))
        .expect("an OpenSIPS source tree has modules/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// POSITIVE CONTROL. Everything else here is phrased as "absent from
/// the C tables means X". If the extraction quietly stopped matching,
/// every such rule would pass while proving nothing. These floors are
/// what makes that failure loud.
///
/// Measured against 4.0.1: 193 module directories, 178 of which
/// declare at least one parameter. Fifteen declare none — the
/// function-only modules such as `textops`, `xml` and `uuid`, plus six
/// like `media_exchange` whose table holds only its `{0, 0, 0}`
/// terminator. Counting table *declarations* gives 184 and counting
/// modules that export a parameter gives 178; this floor tracks the
/// latter, which is what the catalogue is built from.
#[test]
fn the_extraction_still_reads_the_tree() {
    let root = tree();
    let root = Path::new(&root);
    let dirs = module_dirs(root);
    assert!(
        dirs.len() > 150,
        "suspiciously few module directories: {}",
        dirs.len()
    );

    let mut with_tables = 0usize;
    let mut total = 0usize;
    for dir in &dirs {
        let found = param_names_from_c(dir, root);
        if !found.names.is_empty() {
            with_tables += 1;
            total += found.names.len();
        }
    }
    eprintln!(
        "{with_tables}/{} modules expose a param_export_t table, {total} names",
        dirs.len()
    );
    assert!(
        with_tables >= 175,
        "only {with_tables} modules parsed a table; the extraction has regressed"
    );
    assert!(
        total >= 1600,
        "only {total} parameter names extracted; the extraction has regressed"
    );
}

/// POSITIVE CONTROL. A floor on counts survives a parser that returns
/// plausible rubbish, so name the specific parameters too.
#[test]
fn known_parameters_are_extracted() {
    let root = tree();
    let root = Path::new(&root);
    for (module, param) in [
        ("rtpengine", "rtpengine_disable_tout"),
        ("rtpengine", "extra_failover_error"),
        ("tcp_mgm", "connect_timeout_col"),
        ("usrloc", "user_column"),
        ("dialog", "table_name"),
    ] {
        let found = param_names_from_c(&root.join("modules").join(module), root);
        assert!(
            found.names.iter().any(|n| n == param),
            "{module}::{param} must come out of the C tables, got {} names",
            found.names.len()
        );
    }
}

/// `registrar` and `mid_registrar` splice seven shared parameters in
/// through the `reg_modparams` macro in `lib/reg/common.h`. Reading
/// the table without expanding it reports those seven as absent, and
/// the reconciliation would then delete them from the catalogue as
/// phantoms — a false warning on a correct configuration.
#[test]
fn macro_spliced_parameters_are_expanded() {
    let root = tree();
    let root = Path::new(&root);
    for module in ["registrar", "mid_registrar"] {
        let found = param_names_from_c(&root.join("modules").join(module), root);
        assert!(found.complete, "{module}: table did not fully resolve");
        for param in ["max_contacts", "max_aor_len", "expires_max_deviation"] {
            assert!(
                found.names.iter().any(|n| n == param),
                "{module}::{param} comes from reg_modparams and must survive expansion"
            );
        }
    }
}

/// No catalogue entry names a parameter the module never exported.
///
/// This is the false-positive direction: a heading that misnames a
/// parameter used to put an entry in the catalogue for something
/// `modparam()` would reject at startup.
#[test]
fn the_catalogue_contains_no_parameter_absent_from_the_c_tables() {
    let root = tree();
    let root = Path::new(&root);
    let catalogue = builtin_modules();

    let mut phantoms: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for dir in module_dirs(root) {
        let module = dir.file_name().unwrap().to_string_lossy().into_owned();
        let found = param_names_from_c(&dir, root);
        // only modules the reconciliation treats as authoritative: an
        // empty or unresolved parse holds the README harvest as-is
        if found.names.is_empty() || !found.complete {
            continue;
        }
        let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) else {
            continue;
        };
        for p in &doc.params {
            checked += 1;
            if !found.names.contains(&p.name) {
                phantoms.push(format!("{module}::{}", p.name));
            }
        }
    }
    assert!(
        checked > 1000,
        "suspiciously few entries checked: {checked}"
    );
    assert!(
        phantoms.is_empty(),
        "{} catalogue entr(ies) name a parameter absent from the module's C tables:\n{}",
        phantoms.len(),
        phantoms.join("\n")
    );
    eprintln!("{checked} catalogue entries all trace to a C parameter table");
}

/// A parameter the C table exports and no heading documents is in the
/// catalogue, carrying a doc line that says where it came from.
#[test]
fn undocumented_parameters_are_harvested_from_c() {
    let catalogue = builtin_modules();
    for (module, param) in [
        ("rtpengine", "extra_failover_error"),
        ("tcp_mgm", "connect_timeout_col"),
    ] {
        let doc = catalogue
            .modules
            .iter()
            .find(|m| m.name == module)
            .unwrap_or_else(|| panic!("{module} must be in the catalogue"));
        let item = doc
            .params
            .iter()
            .find(|p| p.name == param)
            .unwrap_or_else(|| panic!("{module}::{param} must be harvested from the C table"));
        assert!(
            item.doc.contains("not documented in the module README"),
            "{module}::{param} should say it is undocumented, got {:?}",
            item.doc
        );
    }
}

/// A module with no parameter table keeps its README harvest exactly.
/// The function-only modules are the reason the reconciliation is
/// conditional rather than unconditional.
#[test]
fn modules_without_a_table_are_left_alone() {
    let root = tree();
    let root = Path::new(&root);
    let catalogue = builtin_modules();

    let mut seen = 0usize;
    for module in ["textops", "sipmsgops", "uuid", "xml"] {
        let dir = root.join("modules").join(module);
        if !dir.is_dir() {
            continue;
        }
        seen += 1;
        let found = param_names_from_c(&dir, root);
        assert!(
            found.names.is_empty(),
            "{module} was expected to export no parameters, got {:?}",
            found.names
        );
        if let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) {
            assert!(
                doc.params.is_empty(),
                "{module} has no C table, so the catalogue must show what its README shows"
            );
        }
    }
    assert!(seen >= 3, "the fixture modules are missing from the tree");
}

/// The live path. `builtin_modules()` reads a JSON file generated
/// ahead of time, so a reconciliation that stopped running would leave
/// that file — and every test reading it — looking correct. This
/// exercises `harvest_tree` itself, which is what a user pointing
/// `opensipsLsp.opensipsSrc` at their own tree gets.
#[test]
fn harvesting_the_tree_directly_reconciles_against_c() {
    let root = tree();
    let root = Path::new(&root);
    let harvested = opensips_lsp::catalog::harvest_tree(root);
    assert!(
        harvested.len() > 150,
        "suspiciously few modules harvested: {}",
        harvested.len()
    );
    let by_name = |m: &str| {
        harvested
            .iter()
            .find(|d| d.name == m)
            .unwrap_or_else(|| panic!("{m} must be harvested"))
            .clone()
    };

    // the macro-spliced parameters survive, so the completeness guard
    // is doing its job
    for module in ["registrar", "mid_registrar"] {
        assert!(
            by_name(module)
                .params
                .iter()
                .any(|p| p.name == "max_contacts"),
            "{module}::max_contacts comes from reg_modparams and must survive harvesting"
        );
    }
    // a C-only parameter is added
    assert!(
        by_name("rtpengine")
            .params
            .iter()
            .any(|p| p.name == "extra_failover_error"),
        "rtpengine::extra_failover_error must be harvested from the C table"
    );
    // and a phantom is gone
    assert!(
        !by_name("tls_mgm")
            .params
            .iter()
            .any(|p| p.name == "cipher_list_col"),
        "tls_mgm::cipher_list_col is not exported and must not be harvested"
    );

    let mut phantoms: Vec<String> = Vec::new();
    for doc in &harvested {
        let found = param_names_from_c(&root.join("modules").join(&doc.name), root);
        if found.names.is_empty() || !found.complete {
            continue;
        }
        for p in &doc.params {
            if !found.names.contains(&p.name) {
                phantoms.push(format!("{}::{}", doc.name, p.name));
            }
        }
    }
    assert!(
        phantoms.is_empty(),
        "{} harvested entr(ies) name a parameter absent from the C tables:\n{}",
        phantoms.len(),
        phantoms.join("\n")
    );
}

/// The completeness guard, on a fixture that can actually violate it.
///
/// No module in the 4.0.1 tree parses incompletely, so nothing there
/// exercises this branch: a test that only reads the real tree passes
/// whether the guard is right or wrong. `registrar` is the reason the
/// guard exists — a table splicing a macro this parser cannot find
/// yields a short name list, and dropping against a short list deletes
/// real parameters. These two synthetic modules differ only in whether
/// the splice resolves.
#[test]
fn an_unresolved_splice_drops_nothing() {
    let dir = std::env::temp_dir().join(format!("oslsp-cguard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let readme = "# m\n\n### Exported Parameters\n\n#### documented_only (integer)\n\nIn the README, never exported.\n\n#### really_exported (integer)\n\nIn both.\n";
    for (module, table) in [
        // splices something the parser cannot resolve
        (
            "unresolved",
            "\t{\"really_exported\", INT_PARAM, &x},\n\tSOME_SHARED_MACRO,\n",
        ),
        // resolves fully
        ("resolved", "\t{\"really_exported\", INT_PARAM, &x},\n"),
    ] {
        let md = dir.join("modules").join(module);
        std::fs::create_dir_all(&md).expect("fixture dirs");
        std::fs::write(md.join("README.md"), readme).expect("fixture README");
        std::fs::write(
            md.join("mod.c"),
            format!("static const param_export_t params[] = {{\n{table}\t{{0, 0, 0}}\n}};\n"),
        )
        .expect("fixture source");
    }

    let harvested = opensips_lsp::catalog::harvest_tree(&dir);
    let params = |m: &str| {
        harvested
            .iter()
            .find(|d| d.name == m)
            .unwrap_or_else(|| panic!("{m} must be harvested"))
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
    };

    let unresolved = params("unresolved");
    assert!(
        unresolved.iter().any(|n| n == "documented_only"),
        "an unresolved splice means the name list may be short, so nothing may be dropped; got {unresolved:?}"
    );
    let resolved = params("resolved");
    assert!(
        !resolved.iter().any(|n| n == "documented_only"),
        "a fully resolved table is authoritative and the phantom must go; got {resolved:?}"
    );
    assert!(
        resolved.iter().any(|n| n == "really_exported"),
        "the exported parameter must survive; got {resolved:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// A conditional entry must not make a table look unresolved.
///
/// `rr` guards `ignore_user` behind `ENABLE_USER_CHECK`. Read without
/// skipping the directive, `ifdef` parses as a bare identifier — a
/// macro splice this parser cannot find — and the table is marked
/// incomplete. Every name is still collected, so a test that only
/// checks names passes; what is lost is the authority to drop a
/// phantom from that module.
#[test]
fn conditional_tables_still_resolve_completely() {
    let root = tree();
    let root = Path::new(&root);
    let dir = root.join("modules").join("rr");
    let found = param_names_from_c(&dir, root);
    assert!(
        found.names.iter().any(|n| n == "ignore_user"),
        "rr::ignore_user sits behind a `#ifdef` and must still be collected, got {:?}",
        found.names
    );
    assert!(
        found.complete,
        "rr: a `#ifdef` inside the table must not read as an unresolved splice"
    );
}
