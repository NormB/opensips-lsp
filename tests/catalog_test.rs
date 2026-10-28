//! `catalog`: module documentation harvested from DocBook admin guides
//! and from an OpenSIPS source tree.

mod common;

use opensips_lsp::catalog;
use std::path::Path;

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
    let m = catalog::parse_admin_xml("cachedb_nats", FIXTURE).expect("fixture must parse");
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
    let m = catalog::parse_admin_xml("cachedb_nats", FIXTURE).unwrap();
    assert!(
        m.params[1].doc.contains(r"C:\nats\store"),
        "the Windows path in the fixture doc must keep its backslashes: {:?}",
        m.params[1].doc
    );
}

#[test]
fn empty_input_is_error_not_panic() {
    assert!(
        catalog::parse_admin_xml("m", "").is_err(),
        "an empty admin XML has no chapter, so parsing must return an error instead of a module"
    );
}

#[test]
fn malformed_xml_is_error_not_panic() {
    assert!(
        catalog::parse_admin_xml("m", "<chapter><section id=").is_err(),
        "a truncated tag is malformed XML, so parsing must return an error instead of panicking"
    );
}

#[test]
fn embedded_nul_is_error_not_panic() {
    let s = format!("<chapter>{}</chapter>", '\0');
    assert!(
        catalog::parse_admin_xml("m", &s).is_err(),
        "a NUL character is not allowed in XML, so parsing must return an error: {s:?}"
    );
}

#[test]
fn unknown_entities_do_not_kill_the_parse() {
    let xml = r#"<chapter><section id="param_x" xreflabel="x">
      <title><varname>x</varname> (string)</title>
      <para>Uses &totallymadeup; entity and &amp; a real one.</para>
    </section></chapter>"#;
    let m = catalog::parse_admin_xml("m", xml).expect("unknown entity must not be fatal");
    assert_eq!(m.params[0].name, "x");
    assert!(
        m.params[0].doc.contains("& a real one"),
        "the known &amp; entity must still decode when an unknown entity precedes it: {:?}",
        m.params[0].doc
    );
}

#[test]
fn harvests_the_real_opensips_tree_when_present() {
    // point OPENSIPS_LSP_TEST_TREE at an OpenSIPS source tree to run
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let root = Path::new(&tree);
    assert!(
        root.join("modules").is_dir(),
        "OPENSIPS_LSP_TEST_TREE points at {}, which is not an OpenSIPS \
         source tree",
        root.display()
    );
    let mods = opensips_lsp::catalog::harvest_tree(root);
    assert!(
        mods.len() > 100,
        "expected >100 documented modules, got {}",
        mods.len()
    );
    // tm exists in every OpenSIPS tree
    let tm = mods.iter().find(|m| m.name == "tm").expect("tm doc");
    assert!(
        tm.params.iter().any(|p| p.name == "fr_timeout"),
        "every OpenSIPS tree documents the tm parameter fr_timeout, so the harvest must find it: {:?}",
        tm.params
    );
    assert!(
        tm.functions.iter().any(|f| f.name == "t_relay"),
        "every OpenSIPS tree documents the tm function t_relay, so the harvest must find it: {:?}",
        tm.functions
    );
    // on feature trees carrying cachedb_nats, spot-check it too
    if let Some(cn) = mods.iter().find(|m| m.name == "cachedb_nats") {
        assert!(
            cn.params.iter().any(|p| p.name == "kv_bucket"),
            "cachedb_nats documents kv_bucket, so the harvest must find it: {:?}",
            cn.params
        );
        // the FTS split moved this away
        assert!(
            !cn.params.iter().any(|p| p.name == "enable_search_index"),
            "the FTS split moved enable_search_index out of cachedb_nats, so it must not be harvested there: {:?}",
            cn.params
        );
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
    // silently ejects entries from the harvested section).
    // The list is DERIVED from the server, not written out here.  A
    // hand-maintained list cannot fail for the one thing it exists to
    // catch: this check carried a literal array written before inlay
    // hints existed, so new settings went undocumented while it
    // reported green.
    //
    // Whitespace is stripped before scanning because rustfmt wraps the
    // call as `opts\n.get("name")`, and an extraction looking for the
    // literal `opts.get("` silently finds nothing there — a gate
    // defeated by the formatter CI itself enforces.
    let server = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/server.rs"))
        .expect("src/server.rs");
    let flat: String = server.chars().filter(|c| !c.is_whitespace()).collect();
    let mut opts: Vec<String> = Vec::new();
    for (i, _) in flat.match_indices("opts.get(\"") {
        if let Some(name) = flat[i + 10..].split('"').next()
            && name != "opensipsLsp"
            && !opts.iter().any(|o| o == name)
        {
            opts.push(name.to_string());
        }
    }
    assert!(
        opts.len() >= 8,
        "only {} init options found — the scan went blind: {opts:?}",
        opts.len()
    );
    for opt in &opts {
        assert!(
            params.contains(&opt.as_str()),
            "{opt} is read by the server but missing from docs/ADMIN.md params: {params:?}"
        );
    }
    assert!(
        m.params
            .iter()
            .all(|p| !p.doc.is_empty() && !p.detail.is_empty()),
        "every parameter in docs/ADMIN.md must carry both a doc text and a detail line \
         for hover to show: {:?}",
        m.params
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
    let modules = opensips_lsp::catalog::harvest_tree(Path::new("/nonexistent/tree-xyz"));
    assert!(
        modules.is_empty(),
        "a tree path that does not exist has no module docs to harvest: {modules:?}"
    );
    let core = opensips_lsp::catalog::harvest_core(Path::new("/nonexistent/tree-xyz"));
    assert!(
        core.functions.is_empty() && core.params.is_empty() && core.pvars.is_empty(),
        "a tree path that does not exist has no core docs to harvest: functions {:?}, \
         params {:?}, pvars {:?}",
        core.functions,
        core.params,
        core.pvars
    );
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
    assert!(
        names.contains(&"real_param"),
        "the heading under Exported Parameters is a real item and must be parsed: {names:?}"
    );
    assert!(
        !names.contains(&"fr_timer"),
        "ToC lines are not items: {names:?}"
    );
}

/// The docbook form of the same defect, as the 3.5.9 docs write it.
///
/// A `<title>` holding two `<function>` elements documents both, and
/// a section directly under "Exported Functions" is a function
/// whatever its `id` says: `rtpproxy_stream2xxx`, `ldap-search-fn`
/// and the id-less `lua` and `h350` sections were dropped because
/// only ids starting `func_` were read, which left 24 documented
/// functions out of the 3.5.9 catalogue.
#[test]
fn every_function_an_exported_functions_section_documents_is_harvested() {
    let xml = r#"<chapter>
  <section id="exported_functions">
    <title>Exported Functions</title>
    <section id="func_uac_replace_from" xreflabel="uac_replace_from()">
      <title>
        <function moreinfo="none">uac_replace_from([display],uri)</function>
        <function moreinfo="none">uac_replace_to([display],uri)</function>
      </title>
      <para>Replace the display name or the URI.</para>
    </section>
    <section id="rtpproxy_stream2xxx">
      <title>
        <function>rtpproxy_stream2uac(prompt_name, count)</function>,
        <function>rtpproxy_stream2uas(prompt_name, count)</function>
      </title>
      <para>Stream a prompt.</para>
    </section>
    <section>
      <title>lua_exec(func, [param])</title>
      <para>Run a Lua function.</para>
    </section>
    <section id="exported_pseudo_variables">
      <title>Exported Pseudo-Variables</title>
      <para>Not a function.</para>
    </section>
  </section>
  <section id="exported_mi_functions">
    <title>Exported MI Functions</title>
    <section id="mi_reload">
      <title><function>ds_reload</function></title>
      <para>An MI command, not a script function.</para>
    </section>
  </section>
</chapter>
"#;
    let m = catalog::parse_admin_xml("m", xml).unwrap();
    let got: Vec<(&str, &str, &str)> = m
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f.detail.as_str(), f.doc.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "uac_replace_from",
                "uac_replace_from([display],uri)",
                "Replace the display name or the URI."
            ),
            (
                "uac_replace_to",
                "uac_replace_to([display],uri)",
                "Replace the display name or the URI."
            ),
            (
                "rtpproxy_stream2uac",
                "rtpproxy_stream2uac(prompt_name, count)",
                "Stream a prompt."
            ),
            (
                "rtpproxy_stream2uas",
                "rtpproxy_stream2uas(prompt_name, count)",
                "Stream a prompt."
            ),
            ("lua_exec", "lua_exec(func, [param])", "Run a Lua function."),
        ]
    );
}

/// A section's `id` is not the function's name when the title says
/// otherwise. The 3.5.9 dialog docs file `get_dialogs_by_val` under
/// `func_get_dialog_by_val`; read from the id, the catalogue offered
/// `get_dialog_by_val`, which no OpenSIPS release exports. The id is
/// still the name when the title names no function.
#[test]
fn the_title_names_the_function_and_the_id_is_a_fallback() {
    let xml = r#"<chapter>
  <section id="func_get_dialog_by_val">
    <title><function moreinfo="none">get_dialogs_by_val(name,value,out_avp,out_dlg_no)</function></title>
    <para>Look dialogs up by value.</para>
  </section>
  <section id="func_t_relay">
    <title>Relaying</title>
    <para>No signature in the title.</para>
  </section>
</chapter>
"#;
    let m = catalog::parse_admin_xml("m", xml).unwrap();
    let names: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["get_dialogs_by_val", "t_relay"]);
}

/// A `cmd_export_t` entry whose handler is a documented function's
/// handler, under a name the docs never use, is that function under a
/// second name. sipmsgops exports `has_body` and `has_body_part` both
/// with `has_body_f`, and documents only `has_body_part`.
#[test]
fn an_undocumented_export_sharing_a_documented_handler_is_an_alias() {
    use opensips_lsp::catalog;
    use opensips_lsp::catalog::Item;
    let c = r#"
static const cmd_export_t cmds[]={
	{"has_body",         (cmd_function)has_body_f, {
		{CMD_PARAM_STR|CMD_PARAM_OPT, fixup_mime_type, 0},
		{0, 0, 0}},
		REQUEST_ROUTE|ONREPLY_ROUTE},
	{"has_body_part",    (cmd_function)has_body_f, {
		{CMD_PARAM_STR|CMD_PARAM_OPT, fixup_mime_type, 0},
		{0, 0, 0}},
		REQUEST_ROUTE|ONREPLY_ROUTE},
	{"is_privacy",       (cmd_function)is_privacy_f, {{0, 0, 0}}, REQUEST_ROUTE},
	/* {"old_name",      (cmd_function)is_privacy_f, {{0, 0, 0}}, 0}, */
	{"undocumented",     (cmd_function)other_f, {{0, 0, 0}}, REQUEST_ROUTE},
	{0,0,{{0,0,0}},0}
};
"#;
    let documented = vec![
        Item {
            name: "has_body_part".into(),
            detail: "has_body_part([mime])".into(),
            doc: "Check the body part.".into(),
        },
        Item {
            name: "is_privacy".into(),
            detail: "is_privacy(privacy_type)".into(),
            doc: "Check Privacy.".into(),
        },
    ];
    let got: Vec<(String, String, String)> = catalog::function_aliases_from_c(c, &documented)
        .into_iter()
        .map(|i| (i.name, i.detail, i.doc))
        .collect();
    // the negative controls: a commented-out entry is not exported, and
    // an export with a handler of its own is not anybody's alias
    assert_eq!(
        got,
        vec![(
            "has_body".to_string(),
            "has_body([mime])".to_string(),
            "Alias of `has_body_part`: the C sources export both names with the same \
             handler. Check the body part."
                .to_string()
        )]
    );
}

/// A section whose id names nothing, `param_` or `func_`, gives no item:
/// an empty name would be offered as a completion. The same cause the
/// `readme` fuzz target found in the README harvester.
#[test]
fn a_docbook_section_whose_id_names_nothing_gives_no_item() {
    let xml = r#"<chapter>
<section id="exported_parameters"><title>Exported Parameters</title>
<section id="param_"><title>(integer)</title><para>Nothing.</para></section>
<section id="param_real"><title><varname>real</varname> (integer)</title><para>Real.</para></section>
</section>
<section id="exported_functions"><title>Exported Functions</title>
<section id="func_"><title></title><para>Nothing.</para></section>
<section id="func_go"><title><function>go()</function></title><para>Go.</para></section>
</section>
</chapter>"#;
    let m = catalog::parse_admin_xml("x", xml).expect("parses");
    let params: Vec<&str> = m.params.iter().map(|p| p.name.as_str()).collect();
    let functions: Vec<&str> = m.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(params, ["real"]);
    assert_eq!(functions, ["go"]);
}
