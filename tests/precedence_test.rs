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
