//! `$var` and `$avp` are variables too.
//!
//! GIVEN a reader typing `$` or hovering `$avp(id)`,
//! WHEN the two variable KINDS are as much part of the language as
//! any of the 119 reference variables beside them,
//! THEN they must complete and hover like the rest.
//!
//! They did neither. `Script-CoreVar.md` documents its reference
//! variables as `### <description> - $name` and the two kinds as `##`
//! prose sections — `## Script variables`, `## AVP variables` — each
//! opening with a `**Naming**:` line. The harvester read only the
//! `###` form, so the two most-used variables in any configuration
//! were the two it did not have.
//!
//! The third `**Naming**:` section, `## Reference Variables`, names
//! `$name`: a placeholder for the hundred `###` entries beneath it,
//! not a variable. Reading it the same way would invent a `$name`
//! that no configuration can ever use, so a section with `###`
//! children is a category and not an entry.

mod common;

use opensips_lsp::catalog::{harvest_core, parse_core_vars_md};

const PAGE: &str = r#"# Core variables

## Script variables

**Naming**: `$var(name)`

Attached to the script, persistent for the whole top route.

Also initialize them before first use.

## AVP variables

**Naming**: `$avp(name)` or `$(avp(name)[N])`

Attached to a message or transaction.

## Reference Variables

**Naming**: `$name`

They provide access to information from the SIP message.

### URI in Request's P-Asserted-Identity header - $ai

`$ai` - reference to the URI in the request's P-Asserted-Identity header.
"#;

#[test]
fn a_naming_section_becomes_the_variable_it_names() {
    let vars = parse_core_vars_md(PAGE).expect("the page parses");
    let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    assert!(names.contains(&"$var"), "$var missing: {names:?}");
    assert!(names.contains(&"$avp"), "$avp missing: {names:?}");
    assert!(
        names.contains(&"$ai"),
        "and the `###` entries must still be read: {names:?}"
    );
}

#[test]
fn a_naming_section_with_children_is_a_category_not_a_variable() {
    let vars = parse_core_vars_md(PAGE).expect("the page parses");
    let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
    assert!(
        !names.contains(&"$name"),
        "`$name` is the placeholder standing for the entries below it, not a \
         variable anyone can write: {names:?}"
    );
}

#[test]
fn the_variable_doc_is_the_prose_and_not_just_the_naming_line() {
    let vars = parse_core_vars_md(PAGE).expect("the page parses");
    let var = vars.iter().find(|v| v.name == "$var").unwrap();
    assert!(
        var.doc.contains("persistent for the whole top route"),
        "a hover showing only the naming line repeats what the reader just \
         typed: {:?}",
        var.doc
    );
    let avp = vars.iter().find(|v| v.name == "$avp").unwrap();
    assert!(
        !avp.doc.contains("persistent for the whole top route"),
        "and it stops at the next heading: {:?}",
        avp.doc
    );
}

#[test]
fn the_naming_form_survives_in_the_detail() {
    let vars = parse_core_vars_md(PAGE).expect("the page parses");
    let avp = vars.iter().find(|v| v.name == "$avp").unwrap();
    assert!(
        avp.detail.contains("$avp(name)"),
        "the call form is what a reader needs to write it: {:?}",
        avp.detail
    );
}

/// A malformed naming line must yield NO entry — asserting only that
/// the parser does not panic passes just as well when it invents a
/// variable called `$`, which then sits in every completion list.
#[test]
fn a_malformed_naming_line_yields_no_variable() {
    for md in [
        "## X\n\n**Naming**: ``\n\ndoc\n",
        "## X\n\n**Naming**: `$`\n\ndoc\n",
        "## X\n\n**Naming**: `$(name)`\n\ndoc\n",
        "## X\n\n**Naming**:\n\ndoc\n",
        "## \n\n**Naming**: `$`\n\ndoc\n",
    ] {
        let got = parse_core_vars_md(md).expect("parses");
        assert!(
            got.is_empty(),
            "{md:?} names no variable, yet yielded {:?}",
            got.iter().map(|i| &i.name).collect::<Vec<_>>()
        );
    }
    // POSITIVE CONTROL: the same shape, well formed, does yield one —
    // so the assertions above are not passing on a parser that reads
    // nothing at all.
    let ok = parse_core_vars_md("## X\n\n**Naming**: `$var(name)`\n\ndoc\n").expect("parses");
    assert_eq!(ok.len(), 1, "{ok:?}");
}

/// The pinned tree, not a fixture: upstream's real page.
#[test]
fn the_real_page_documents_both_variable_kinds() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    let names: Vec<&str> = core.pvars.iter().map(|v| v.name.as_str()).collect();

    // POSITIVE CONTROL: an empty harvest must not read as two misses.
    assert!(
        names.len() > 50,
        "only {} variables harvested: {names:?}",
        names.len()
    );
    for want in ["$var", "$avp"] {
        assert!(names.contains(&want), "{want} not harvested: {names:?}");
        let it = core.pvars.iter().find(|v| v.name == want).unwrap();
        assert!(!it.doc.trim().is_empty(), "{want} has an empty summary");
    }
    assert!(
        !names.contains(&"$name"),
        "the reference-variable category must not become an entry: {names:?}"
    );
}

/// The shipped catalogue carries them, so they work with no checkout.
#[test]
fn the_built_in_catalogue_carries_both_variable_kinds() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let names: Vec<&str> = core.pvars.iter().map(|v| v.name.as_str()).collect();
    for want in ["$var", "$avp"] {
        assert!(names.contains(&want), "{want} missing: {names:?}");
    }
}

#[test]
fn hovering_avp_and_var_answers_with_their_documentation() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let body = "route {\n    $var(a) = 1;\n    $avp(b) = $var(a);\n}\n";
    for (word, line, col) in [("$var", 1u32, 4u32), ("$avp", 2, 4)] {
        let got = opensips_lsp::logic::hover_markdown_at(&[], core, body, word, line, col)
            .unwrap_or_else(|| panic!("{word} must hover"));
        assert!(got.contains(word), "the hover must name it: {got:?}");
        assert!(
            got.len() > word.len() + 8,
            "and carry documentation, not just the name: {got:?}"
        );
    }
}

#[test]
fn typing_a_dollar_offers_both_variable_kinds() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let offered =
        opensips_lsp::logic::completions_with_core(&[], core, "route {\n    $\n}\n", "    $");
    let labels: Vec<&str> = offered.iter().map(|c| c.label.as_str()).collect();
    for want in ["$var", "$avp"] {
        assert!(
            labels.contains(&want),
            "{want} is not offered after `$`: {} labels",
            labels.len()
        );
    }
}
