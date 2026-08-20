use opensips_lsp::catalog::parse_admin_xml;

const FIXTURE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<chapter>
  <title>&adminguide;</title>
  <section id="param_kv_bucket" xreflabel="kv_bucket">
    <title><varname>kv_bucket</varname> (string)</title>
    <para>
      The name of the NATS JetStream KV bucket that &osips; will use
      for storage.
    </para>
    <para>Second paragraph must NOT be part of the doc summary.</para>
  </section>
  <section id="param_kv_replicas" xreflabel="kv_replicas">
    <title><varname>kv_replicas</varname> (integer)</title>
    <para>Replica count, path C:\nats\store style backslashes stay intact.</para>
  </section>
  <section id="func_nats_kv_get" xreflabel="nats_kv_get()">
    <title>
      <function moreinfo="none">nats_kv_get(bucket, key, value_pvar, [revision_pvar])</function>
    </title>
    <para>Retrieves a value from the bucket.</para>
  </section>
</chapter>
"#;

#[test]
fn parses_params_and_functions() {
    let m = parse_admin_xml("cachedb_nats", FIXTURE).expect("fixture must parse");
    assert_eq!(m.name, "cachedb_nats");
    assert_eq!(m.params.len(), 2);
    assert_eq!(m.functions.len(), 1);

    let p = &m.params[0];
    assert_eq!(p.name, "kv_bucket");
    assert_eq!(p.detail, "string");
    // whitespace collapsed, entity neutralized, second para excluded
    assert_eq!(
        p.doc,
        "The name of the NATS JetStream KV bucket that OpenSIPS will use for storage."
    );

    let f = &m.functions[0];
    assert_eq!(f.name, "nats_kv_get");
    assert_eq!(
        f.detail,
        "nats_kv_get(bucket, key, value_pvar, [revision_pvar])"
    );
    assert_eq!(f.doc, "Retrieves a value from the bucket.");
}

#[test]
fn backslashes_survive() {
    let m = parse_admin_xml("cachedb_nats", FIXTURE).unwrap();
    assert!(m.params[1].doc.contains(r"C:\nats\store"));
}

#[test]
fn empty_input_is_error_not_panic() {
    assert!(parse_admin_xml("m", "").is_err());
}

#[test]
fn malformed_xml_is_error_not_panic() {
    assert!(parse_admin_xml("m", "<chapter><section id=").is_err());
}

#[test]
fn embedded_nul_is_error_not_panic() {
    let s = format!("<chapter>{}</chapter>", '\0');
    assert!(parse_admin_xml("m", &s).is_err());
}

#[test]
fn unknown_entities_do_not_kill_the_parse() {
    let xml = r#"<chapter><section id="param_x" xreflabel="x">
      <title><varname>x</varname> (string)</title>
      <para>Uses &totallymadeup; entity and &amp; a real one.</para>
    </section></chapter>"#;
    let m = parse_admin_xml("m", xml).expect("unknown entity must not be fatal");
    assert_eq!(m.params[0].name, "x");
    assert!(m.params[0].doc.contains("& a real one"));
}

#[test]
fn harvests_the_real_opensips_tree_when_present() {
    // point OPENSIPS_LSP_TEST_TREE at an OpenSIPS source tree to run
    let Ok(tree) = std::env::var("OPENSIPS_LSP_TEST_TREE") else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_TREE not set");
        return;
    };
    let root = std::path::Path::new(&tree);
    if !root.join("modules").is_dir() {
        eprintln!("SKIP: no opensips tree at {}", root.display());
        return;
    }
    let mods = opensips_lsp::catalog::harvest_tree(root);
    assert!(
        mods.len() > 100,
        "expected >100 documented modules, got {}",
        mods.len()
    );
    // tm exists in every OpenSIPS tree
    let tm = mods.iter().find(|m| m.name == "tm").expect("tm doc");
    assert!(tm.params.iter().any(|p| p.name == "fr_timeout"));
    assert!(tm.functions.iter().any(|f| f.name == "t_relay"));
    // on feature trees carrying cachedb_nats, spot-check it too
    if let Some(cn) = mods.iter().find(|m| m.name == "cachedb_nats") {
        assert!(cn.params.iter().any(|p| p.name == "kv_bucket"));
        // the FTS split moved this away
        assert!(!cn.params.iter().any(|p| p.name == "enable_search_index"));
    }
}

#[test]
fn the_shipped_admin_doc_follows_the_opensips_readme_standard() {
    // docs/ADMIN.md is written in the OpenSIPS 4.x module-README
    // structure; our own harvester must be able to parse it.
    let md = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/ADMIN.md"))
        .expect("docs/ADMIN.md must exist");
    let m = opensips_lsp::catalog::parse_readme_md("opensips-lsp", &md).expect("must parse");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    // EVERY initialization option the server reads must be documented
    // inside the Exported Parameters section (a misplaced heading
    // silently ejects entries from the harvested section)
    for opt in [
        "opensipsPath",
        "opensipsSrc",
        "checkTimeoutMs",
        "analyzerDiagnostics",
        "maxDiagnostics",
        "snippetCompletions",
        "cacheDir",
    ] {
        assert!(
            params.contains(&opt),
            "{opt} missing from params: {params:?}"
        );
    }
    assert!(
        m.params
            .iter()
            .all(|p| !p.doc.is_empty() && !p.detail.is_empty())
    );
    // ...and every env var must appear somewhere in the guide
    for env in [
        "OPENSIPS_LSP_BIN",
        "OPENSIPS_LSP_SRC",
        "OPENSIPS_LSP_CHECK_TIMEOUT_MS",
        "OPENSIPS_LSP_CACHE_DIR",
        "OPENSIPS_LSP_OUTPUT_CAP_BYTES",
        "OPENSIPS_LSP_ANALYZER_DEBOUNCE_MS",
    ] {
        assert!(md.contains(env), "{env} undocumented in ADMIN.md");
    }
}

#[test]
fn hostile_doc_content_is_sanitized_at_harvest() {
    let md = r#"# evil

### Exported Parameters

#### bad_param (string)

Text with <img src=x onerror=alert(1)> raw html, a
[command link](command:workbench.action.evil), a
[js link](javascript:alert(1)), and a
[good link](https://example.com/doc) that stays.
"#;
    let m = opensips_lsp::catalog::parse_readme_md("evil", md).unwrap();
    let doc = &m.params[0].doc;
    assert!(
        !doc.contains('<') && !doc.contains('>'),
        "raw html must be stripped: {doc}"
    );
    assert!(
        !doc.contains("command:"),
        "command links must be neutralized: {doc}"
    );
    assert!(
        !doc.contains("javascript:"),
        "js links must be neutralized: {doc}"
    );
    assert!(doc.contains("command link"), "link labels survive: {doc}");
    assert!(
        doc.contains("https://example.com/doc"),
        "http(s) links survive: {doc}"
    );
}

#[test]
fn missing_tree_harvests_empty() {
    let modules =
        opensips_lsp::catalog::harvest_tree(std::path::Path::new("/nonexistent/tree-xyz"));
    assert!(modules.is_empty());
    let core = opensips_lsp::catalog::harvest_core(std::path::Path::new("/nonexistent/tree-xyz"));
    assert!(core.functions.is_empty() && core.params.is_empty() && core.pvars.is_empty());
}

#[test]
fn readme_parser_survives_adversarial_input() {
    for md in [
        "",
        "\0\0",
        "### Exported Parameters\n#### \\\\ (string)\n\nback\\\\slash\n",
        "### Exported Parameters\n####\n\n\n",
        "#### orphan (int)\n\nno section\n",
    ] {
        let _ = opensips_lsp::catalog::parse_readme_md("m", md);
    }
    // a ToC line that MENTIONS a param heading is not an item
    let md = "## Contents\n- [1.1 fr_timer (integer)](#x)\n\n### Exported Parameters\n\n#### real_param (string)\n\nDoc.\n";
    let m = opensips_lsp::catalog::parse_readme_md("m", md).expect("parses");
    let names: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert!(names.contains(&"real_param"));
    assert!(
        !names.contains(&"fr_timer"),
        "ToC lines are not items: {names:?}"
    );
}
