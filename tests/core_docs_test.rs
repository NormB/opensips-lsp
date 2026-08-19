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
    let Ok(tree) = std::env::var("OPENSIPS_LSP_TEST_TREE") else {
        eprintln!("SKIP: OPENSIPS_LSP_TEST_TREE not set");
        return;
    };
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
