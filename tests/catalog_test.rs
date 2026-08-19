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
    let root = std::path::Path::new("/home/gator/scratch/nats-audit-fixes");
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
    let cn = mods
        .iter()
        .find(|m| m.name == "cachedb_nats")
        .expect("cachedb_nats doc");
    assert!(cn.params.iter().any(|p| p.name == "kv_bucket"));
    assert!(cn.functions.iter().any(|f| f.name == "nats_kv_get"));
    // the FTS split moved this away: it must NOT be a cachedb_nats param
    assert!(!cn.params.iter().any(|p| p.name == "enable_search_index"));
}

#[test]
fn the_shipped_admin_doc_follows_the_opensips_readme_standard() {
    // docs/ADMIN.md is written in the OpenSIPS 4.x module-README
    // structure; our own harvester must be able to parse it.
    let md = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/ADMIN.md"))
        .expect("docs/ADMIN.md must exist");
    let m = opensips_lsp::catalog::parse_readme_md("opensips-lsp", &md).expect("must parse");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    assert!(params.contains(&"opensipsPath"), "params: {params:?}");
    assert!(params.contains(&"opensipsSrc"));
    assert!(params.contains(&"checkTimeoutMs"));
    assert!(
        m.params
            .iter()
            .all(|p| !p.doc.is_empty() && !p.detail.is_empty())
    );
}
