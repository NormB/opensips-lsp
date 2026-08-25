//! Gates that read the REAL OpenSIPS lexer rather than restating it.
//!
//! `inert_directives_test.rs` already does this for the preprocessor —
//! it reads `cfg.lex` and proves OpenSIPS has none. The same
//! technique belongs on the route kinds, which are the part of the
//! language most likely to grow: `cfg.lex` declares them as `ROUTE*`
//! macros, and that declaration is what the real parser accepts.
//!
//! The analyzer carries its own list. A kind upstream adds and this
//! one misses is a route block the server cannot see at all, and
//! every call into it is then reported as undefined — a warning on a
//! configuration that is correct.

mod common;

use opensips_lsp::analyze::route_defs;

/// The route-kind keywords `cfg.lex` declares, from lines of the form
/// `ROUTE_FAILURE failure_route`.
fn kinds_from_the_lexer(lex: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lex.lines() {
        let mut it = line.split_whitespace();
        let (Some(macro_name), Some(keyword)) = (it.next(), it.next()) else {
            continue;
        };
        // exactly two tokens: a macro definition, not a rule
        if it.next().is_some() {
            continue;
        }
        if !macro_name.starts_with("ROUTE") {
            continue;
        }
        if !keyword
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            continue;
        }
        if !out.contains(&keyword.to_string()) {
            out.push(keyword.to_string());
        }
    }
    out
}

#[test]
fn every_route_kind_in_the_real_lexer_is_recognised() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let lex = std::fs::read_to_string(std::path::Path::new(&tree).join("cfg.lex"))
        .expect("cfg.lex in the pinned tree");
    let kinds = kinds_from_the_lexer(&lex);

    // POSITIVE CONTROL. Every assertion below is "for each kind the
    // lexer declares"; if the extraction stopped matching, the loop
    // would run over nothing and pass having proved nothing. 4.0.1
    // declares nine.
    assert!(
        kinds.len() >= 9,
        "only {} route kinds extracted from cfg.lex: {kinds:?}",
        kinds.len()
    );
    assert!(
        kinds.iter().any(|k| k == "event_route"),
        "the extraction must find event_route: {kinds:?}"
    );

    let mut unseen: Vec<String> = Vec::new();
    for kind in &kinds {
        // both shapes the analyzer accepts: a bare block, and a named
        // one. A kind it does not know matches neither.
        let named = format!("{kind}[DRIFT_GATE] {{\n    exit;\n}}\n");
        let bare = format!("{kind} {{\n    exit;\n}}\n");
        let found_named = route_defs(&named).iter().any(|d| d.name == "DRIFT_GATE");
        let found_bare = !route_defs(&bare).is_empty();
        if !found_named && !found_bare {
            unseen.push(kind.clone());
        }
    }
    assert!(
        unseen.is_empty(),
        "cfg.lex declares {} route kind(s) the analyzer does not recognise: {unseen:?}\n\
         (a block of that kind is invisible, and every call into it warns)",
        unseen.len()
    );
    eprintln!("{} route kinds, all recognised: {kinds:?}", kinds.len());
}

/// The reverse direction: the analyzer must not invent a kind the
/// language does not have. A name it treats as a route block but the
/// parser does not is a definition the server believes in and
/// OpenSIPS rejects.
#[test]
fn the_analyzer_recognises_no_route_kind_the_lexer_lacks() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let lex = std::fs::read_to_string(std::path::Path::new(&tree).join("cfg.lex"))
        .expect("cfg.lex in the pinned tree");
    let kinds = kinds_from_the_lexer(&lex);
    assert!(kinds.len() >= 9, "positive control: {kinds:?}");

    // Kamailio's kinds are the realistic way a wrong one gets in:
    // the two languages are close enough that a rule copied across
    // would look right.
    for foreign in [
        "request_route",
        "reply_route",
        "onsend_route",
        "event_route_x",
        "not_a_route",
    ] {
        if kinds.iter().any(|k| k == foreign) {
            continue; // OpenSIPS really does have it
        }
        let named = format!("{foreign}[DRIFT_GATE] {{\n    exit;\n}}\n");
        assert!(
            !route_defs(&named).iter().any(|d| d.name == "DRIFT_GATE"),
            "'{foreign}' is not in cfg.lex but the analyzer reads it as a route definition"
        );
    }
}
