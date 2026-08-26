mod common;

use opensips_lsp::catalog::{parse_core_functions_md, parse_core_params_md, parse_core_vars_md};

const FUNCS: &str = r#"# Core functions

Intro prose.

## add_local_rport()

Adds rport to the Via header.

## assert(statement, [description])

Aborts when the statement is false.
Second line same paragraph.

Second paragraph excluded.

## cache_store(storage_id, attribute, value, [timeout])

Stores a value.
"#;

const PARAMS: &str = r#"# Core parameters

## Core parameters

### abort_on_assert

Boolean; abort instead of shutting down on a failed assert.

```
abort_on_assert = true
```

### advertised_address

Address advertised in Via and Contact, path C:\x kept intact.
"#;

const VARS: &str = r#"# Core variables

## Reference Variables

### URI in SIP Request's P-Asserted-Identity header - $ai

The P-Asserted-Identity URI.

### Request URI - $ru

The full request URI.

### Auth response  - $auth.resp

Digest response.
"#;

#[test]
fn parses_core_functions() {
    let items = parse_core_functions_md(FUNCS).expect("parses");
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["add_local_rport", "assert", "cache_store"]);
    assert_eq!(items[1].detail, "assert(statement, [description])");
    assert!(items[1].doc.starts_with("Aborts when"));
    assert!(items[1].doc.contains("Second line"));
    assert!(!items[1].doc.contains("Second paragraph excluded"));
}

#[test]
fn parses_core_params() {
    let items = parse_core_params_md(PARAMS).expect("parses");
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["abort_on_assert", "advertised_address"]);
    assert!(items[0].doc.starts_with("Boolean"));
    assert!(items[1].doc.contains(r"C:\x"));
}

#[test]
fn parses_core_vars() {
    let items = parse_core_vars_md(VARS).expect("parses");
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["$ai", "$ru", "$auth.resp"]);
    // description half of the heading becomes the detail
    assert_eq!(items[1].detail, "Request URI");
    assert_eq!(items[1].doc, "The full request URI.");
}

#[test]
fn adversarial_core_docs_do_not_panic() {
    for s in ["", "\0", "## (", "### - $", "## x(\n"] {
        let _ = parse_core_functions_md(s);
        let _ = parse_core_params_md(s);
        let _ = parse_core_vars_md(s);
    }
}

#[test]
fn harvests_core_docs_from_a_real_4x_tree_when_present() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = opensips_lsp::catalog::harvest_core(std::path::Path::new(&tree));
    assert!(
        core.functions.len() > 40,
        "core functions: {}",
        core.functions.len()
    );
    assert!(core.params.len() > 80, "core params: {}", core.params.len());
    assert!(core.pvars.len() > 50, "pvars: {}", core.pvars.len());
    assert!(core.functions.iter().any(|f| f.name == "cache_store"));
    assert!(core.params.iter().any(|p| p.name == "advertised_address"));
    assert!(core.pvars.iter().any(|v| v.name == "$ru"));
}

/// Owed 1/2: the core-function heading contract, stated.
///
/// A test fixture written against the wrong shape tests nothing and
/// says so only if something else notices. Mine used `### name` and
/// harvested zero functions; the assertion that caught it was a
/// positive control, not the fixture. The contract is `## name(args)`
/// — level TWO, and parentheses required — and nothing pinned either
/// half.
#[test]
fn the_core_function_heading_is_level_two_and_parenthesised() {
    let names = |md: &str| -> Vec<String> {
        parse_core_functions_md(md)
            .expect("parses")
            .into_iter()
            .map(|i| i.name)
            .collect()
    };
    // POSITIVE CONTROL: the shape that works
    assert_eq!(
        names("# F\n\n## rewriteuri(uri)\n\nRewrites it.\n"),
        vec!["rewriteuri"]
    );
    assert!(
        names("# F\n\n### rewriteuri(uri)\n\nRewrites it.\n").is_empty(),
        "a level-three heading is not a function here — the parameters page uses \
         `###`, and confusing the two harvests nothing at all"
    );
    assert!(
        names("# F\n\n## rewriteuri\n\nRewrites it.\n").is_empty(),
        "a heading with no argument list is not a call signature"
    );
}

/// Owed 2/2: and the real page still satisfies it.
///
/// The failure mode this guards is silence: a contract change
/// upstream harvests zero functions, every hover for a core call goes
/// blank, and no fixture-based test notices because fixtures are
/// written to the shape the code already expects.
#[test]
fn the_real_functions_page_yields_its_functions() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let page = std::fs::read_to_string(
        std::path::Path::new(&tree).join("docs/manual/Script-CoreFunctions.md"),
    )
    .expect("the functions page");
    let items = parse_core_functions_md(&page).expect("parses");

    // the page was read, and it is not empty
    assert!(page.len() > 5_000, "the page read as {} bytes", page.len());
    assert!(
        items.len() >= 40,
        "only {} functions harvested from a {}-byte page",
        items.len(),
        page.len()
    );
    for want in ["xlog", "force_send_socket"] {
        assert!(
            items.iter().any(|i| i.name == want),
            "{want} missing from the real page"
        );
    }
    for i in &items {
        assert!(!i.doc.trim().is_empty(), "{} would hover blank", i.name);
    }
}
