//! Doc-source precedence: 4.x markdown (`modules/*/README.md`) is the
//! most current documentation and must win; docbook
//! (`modules/*/doc/*_admin.xml`) is the fallback for older trees.

use opensips_lsp::catalog::harvest_tree;

fn write(path: &std::path::Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn readme_md_wins_over_docbook_when_both_exist() {
    let root = std::env::temp_dir().join(format!("oslsp-prec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = root.join("modules").join("both");
    write(
        &m.join("README.md"),
        "# both\n\n### Exported Parameters\n\n#### from_markdown (string)\n\nCurrent doc.\n",
    );
    write(
        &m.join("doc").join("both_admin.xml"),
        r#"<chapter><section id="param_from_docbook" xreflabel="x">
             <title><varname>from_docbook</varname> (string)</title>
             <para>Stale doc.</para></section></chapter>"#,
    );
    let mods = harvest_tree(&root);
    let both = mods
        .iter()
        .find(|x| x.name == "both")
        .expect("module harvested");
    let names: Vec<&str> = both.params.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["from_markdown"],
        "README.md must win: {names:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn docbook_is_the_fallback_when_no_readme() {
    let root = std::env::temp_dir().join(format!("oslsp-prec-db-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = root.join("modules").join("legacy");
    write(
        &m.join("doc").join("legacy_admin.xml"),
        r#"<chapter><section id="param_old_param" xreflabel="x">
             <title><varname>old_param</varname> (string)</title>
             <para>Docbook-only doc.</para></section></chapter>"#,
    );
    let mods = harvest_tree(&root);
    let legacy = mods
        .iter()
        .find(|x| x.name == "legacy")
        .expect("module harvested");
    assert_eq!(legacy.params[0].name, "old_param");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn empty_readme_falls_back_to_docbook() {
    // a placeholder README.md must not mask real docbook docs
    let root = std::env::temp_dir().join(format!("oslsp-prec-e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let m = root.join("modules").join("mixed");
    write(
        &m.join("README.md"),
        "# mixed\n\nNo exported sections here.\n",
    );
    write(
        &m.join("doc").join("mixed_admin.xml"),
        r#"<chapter><section id="param_real_param" xreflabel="x">
             <title><varname>real_param</varname> (int)</title>
             <para>Real doc.</para></section></chapter>"#,
    );
    let mods = harvest_tree(&root);
    let mixed = mods.iter().find(|x| x.name == "mixed").expect("harvested");
    assert_eq!(mixed.params[0].name, "real_param");
    let _ = std::fs::remove_dir_all(&root);
}

/// A module that exports nothing is still a module.
///
/// `xml`, `event_datagram`, `db_perlvdb`, `presence_mwi`,
/// `presence_xcapdiff`, `tls_openssl` and `auth_web3` all document
/// their exported sections as `*None*.` — and were dropped from the
/// catalogue entirely, so `loadmodule "` offered seven fewer modules
/// than the tree has.  The empty result is what falls through to
/// docbook; with nothing to fall through to, it is the answer.
#[test]
fn a_module_that_exports_nothing_is_still_in_the_catalogue() {
    let root = std::env::temp_dir().join(format!("oslsp-prec-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write(
        &root.join("modules").join("xml").join("README.md"),
        "# xml\n\n### Exported Parameters\n\n*None*.\n\n### Exported Functions\n\n*None*.\n",
    );
    // and one that still prefers the docbook it does have
    let legacy = root.join("modules").join("legacy");
    write(
        &legacy.join("README.md"),
        "# legacy\n\n### Exported Parameters\n\n*None*.\n",
    );
    write(
        &legacy.join("doc").join("legacy_admin.xml"),
        r#"<chapter><section id="param_old_param" xreflabel="x">
             <title><varname>old_param</varname> (string)</title>
             <para>Docbook-only doc.</para></section></chapter>"#,
    );

    let mods = harvest_tree(&root);
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, vec!["legacy", "xml"], "{names:?}");
    let x = mods.iter().find(|m| m.name == "xml").unwrap();
    assert!(x.params.is_empty() && x.functions.is_empty());
    let l = mods.iter().find(|m| m.name == "legacy").unwrap();
    assert_eq!(l.params[0].name, "old_param", "docbook must still win here");
    let _ = std::fs::remove_dir_all(&root);
}
