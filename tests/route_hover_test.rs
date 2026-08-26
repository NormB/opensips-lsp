//! Hovering a route type says what that route type is.
//!
//! GIVEN `startup_route`, `timer_route` and the rest are the blocks a
//! configuration is built out of,
//! WHEN a reader hovers one,
//! THEN they get its documentation — the same as for a core function
//! or a pseudo-variable.
//!
//! They got nothing. `startup_route` and `timer_route` returned null,
//! and bare `route` answered with the core FUNCTION `route(name, ...)`
//! — the call form, not the block being defined. Upstream documents
//! all nine in `docs/manual/Script-Routes.md`; the harvester read the
//! functions, parameters and variables pages beside it and not that
//! one.

mod common;

use opensips_lsp::catalog::{harvest_core, parse_core_routes_md};

const PAGE: &str = r#"# Routes

## route

The main route, executed for every SIP request.

More detail that is not the summary.

## startup_route

Executed once, at startup, before any SIP traffic is processed.

## timer_route

Executed periodically, at the interval given in its definition.
"#;

#[test]
fn a_routes_page_yields_one_entry_per_heading() {
    let routes = parse_core_routes_md(PAGE).expect("the page parses");
    let names: Vec<&str> = routes.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, vec!["route", "startup_route", "timer_route"]);

    let startup = routes.iter().find(|r| r.name == "startup_route").unwrap();
    assert!(
        startup.doc.starts_with("Executed once, at startup"),
        "the first paragraph is the summary: {:?}",
        startup.doc
    );
    assert!(
        !startup.doc.contains("Executed periodically"),
        "and it stops at the next heading: {:?}",
        startup.doc
    );
    // the first entry's summary must not swallow the paragraph below it
    let main = routes.iter().find(|r| r.name == "route").unwrap();
    assert!(
        !main.doc.contains("not the summary"),
        "only the first paragraph: {:?}",
        main.doc
    );
}

#[test]
fn adversarial_pages_do_not_panic() {
    for md in ["", "\0\0", "## \n\n", "## route", "no headings at all\n"] {
        let _ = parse_core_routes_md(md);
    }
}

/// Every route kind the real lexer declares must be documented, or a
/// reader hovering one of them still gets nothing.
#[test]
fn the_real_tree_documents_every_route_kind() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    let names: Vec<&str> = core.routes.iter().map(|r| r.name.as_str()).collect();

    // POSITIVE CONTROL: an empty harvest would satisfy nothing below,
    // but say so clearly rather than as nine confusing misses.
    assert!(
        names.len() >= 9,
        "only {} route types harvested: {names:?}",
        names.len()
    );
    for kind in [
        "route",
        "branch_route",
        "failure_route",
        "onreply_route",
        "error_route",
        "local_route",
        "startup_route",
        "timer_route",
        "event_route",
    ] {
        assert!(names.contains(&kind), "{kind} is not documented: {names:?}");
        let it = core.routes.iter().find(|r| r.name == kind).unwrap();
        assert!(!it.doc.trim().is_empty(), "{kind} has an empty summary");
    }
}

/// The shipped catalogue carries the route types, so a reader gets
/// them without configuring a source tree.
#[test]
fn the_built_in_catalogue_carries_the_route_types() {
    let core = opensips_lsp::catalog::builtin_core();
    let names: Vec<&str> = core.core.routes.iter().map(|r| r.name.as_str()).collect();
    assert!(
        names.len() >= 9,
        "the vendored core catalogue must carry them: {names:?}"
    );
    for kind in ["startup_route", "timer_route", "event_route"] {
        assert!(names.contains(&kind), "{kind} missing: {names:?}");
    }
}

/// Hover text for a route type, from the catalogue the server holds.
#[test]
fn hovering_a_route_type_answers_with_its_documentation() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    for kind in [
        "startup_route",
        "timer_route",
        "event_route",
        "failure_route",
    ] {
        let body = format!("{kind} {{\n    xlog(\"x\");\n}}\n");
        let got = opensips_lsp::logic::hover_markdown_at(&[], core, &body, kind, 0, 0)
            .unwrap_or_else(|| panic!("{kind} must hover"));
        assert!(
            got.contains(kind),
            "the hover must name the route type: {got:?}"
        );
        assert!(
            got.to_lowercase().contains("route type"),
            "and say what it is: {got:?}"
        );
    }
}

/// Bare `route` is both a route type and a core function. At a block
/// definition the type is what the reader is looking at; the function
/// answer belongs at a call site.
#[test]
fn bare_route_hovers_as_a_type_where_it_defines_a_block() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let definition = "route[MAIN] {\n    xlog(\"x\");\n}\n";
    let got = opensips_lsp::logic::hover_markdown_at(&[], core, definition, "route", 0, 0)
        .expect("route must hover");
    assert!(
        got.to_lowercase().contains("route type"),
        "at a definition it is the block: {got:?}"
    );

    let call = "route {\n    route(MAIN);\n}\n";
    let got = opensips_lsp::logic::hover_markdown_at(&[], core, call, "route", 1, 4)
        .expect("route must hover at a call too");
    assert!(
        got.contains("route(name"),
        "at a call site it is the function: {got:?}"
    );
}

/// Every route kind the real lexer declares must hover.
///
/// The analyzer's kind list is already held against `cfg.lex`. The
/// documentation is a second list, from a different file, and nothing
/// held it against anything: a kind upstream adds is recognised as a
/// block and still hovers nothing, which is the state every kind was
/// in before this work.
#[test]
fn every_route_kind_in_the_lexer_has_documentation() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let lex = std::fs::read_to_string(std::path::Path::new(&tree).join("cfg.lex"))
        .expect("cfg.lex in the pinned tree");

    let mut kinds: Vec<String> = Vec::new();
    for l in lex.lines() {
        let mut it = l.split_whitespace();
        let (Some(m), Some(kw)) = (it.next(), it.next()) else {
            continue;
        };
        if it.next().is_some() || !m.starts_with("ROUTE") {
            continue;
        }
        if kw.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !kinds.contains(&kw.to_string())
        {
            kinds.push(kw.to_string());
        }
    }
    // POSITIVE CONTROL: an extraction that stopped matching would
    // loop over nothing and pass having checked no kind at all.
    assert!(
        kinds.len() >= 9,
        "only {} kinds extracted: {kinds:?}",
        kinds.len()
    );

    let core = &opensips_lsp::catalog::builtin_core().core;
    let undocumented: Vec<&String> = kinds
        .iter()
        .filter(|k| !core.routes.iter().any(|r| &r.name == *k))
        .collect();
    assert!(
        undocumented.is_empty(),
        "cfg.lex declares route kind(s) with no documentation, so hovering them \
         answers nothing: {undocumented:?}"
    );
}

/// A route type with no function of the same name answers wherever it
/// is written.
///
/// Only `route` has a second meaning. The definition check exists to
/// tell those two apart, and applying it to the others would make
/// them silent everywhere except a block header — which is exactly
/// the bug this replaced, in a narrower form.
#[test]
fn a_route_type_with_no_rival_meaning_hovers_anywhere() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    // a line that is NOT a route definition
    let doc = "route {\n    xlog(\"startup_route\");\n}\n";
    for kind in ["startup_route", "timer_route", "local_route"] {
        assert!(
            !core.functions.iter().any(|f| f.name == kind),
            "{kind} must have no rival core function, or this test proves nothing"
        );
        let got = opensips_lsp::logic::hover_markdown_at(&[], core, doc, kind, 1, 10)
            .unwrap_or_else(|| panic!("{kind} must hover away from a definition"));
        assert!(got.to_lowercase().contains("route type"), "{kind}: {got:?}");
    }
}
