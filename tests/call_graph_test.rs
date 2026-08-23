//! The route call graph that call hierarchy is built on.
//!
//! An edge is a `route(NAME)` call. Its target is always a main-table
//! `route[NAME]` block — that is the whole OpenSIPS route-namespace
//! rule — but its *source* can be any route-family block, because a
//! `failure_route` may perfectly well call into the main table.

mod common;
use opensips_lsp::analyze;
use opensips_lsp::logic::{call_edges, enclosing_block};

const CFG: &str = "\
route {
    route(RELAY);
    route(\"QUOTED\");
}
route[RELAY] {
    route(DEEPER);
}
failure_route[RELAY] {
    route(RELAY);
}
route[DEEPER] {
    exit;
}
route[QUOTED] {
    exit;
}
";

#[test]
fn a_call_is_attributed_to_the_block_that_contains_it() {
    let edges = call_edges(CFG);
    let named: Vec<(String, String)> = edges
        .iter()
        .map(|(caller, call)| {
            (
                caller
                    .as_ref()
                    .map(|b| format!("{}[{}]", b.kind, b.name))
                    .unwrap_or_else(|| "<none>".into()),
                call.name.clone(),
            )
        })
        .collect();
    assert_eq!(
        named,
        vec![
            ("route[]".to_string(), "RELAY".to_string()),
            ("route[]".to_string(), "QUOTED".to_string()),
            ("route[RELAY]".to_string(), "DEEPER".to_string()),
            // a failure_route is not callable via route(), but it can
            // still call into the main table: a real edge
            ("failure_route[RELAY]".to_string(), "RELAY".to_string()),
        ]
    );
}

#[test]
fn quoted_and_bare_call_sites_are_both_edges() {
    let edges = call_edges(CFG);
    assert!(
        edges.iter().any(|(_, c)| c.name == "QUOTED"),
        "a quoted route() call is still a call"
    );
}

#[test]
fn enclosing_block_finds_the_containing_route() {
    let blocks = analyze::route_blocks(CFG);
    // line 5 is `route(DEEPER);` inside route[RELAY]
    let b = enclosing_block(&blocks, 5).expect("a block contains line 5");
    assert_eq!(b.name, "RELAY");
    assert_eq!(b.kind, "route");
    // the keyword line of a block is inside it
    let b = enclosing_block(&blocks, 7).expect("a block contains line 7");
    assert_eq!(b.kind, "failure_route");
}

#[test]
fn a_line_outside_every_block_has_no_enclosing_block() {
    let text = "loadmodule \"tm.so\"\nroute {\nexit;\n}\n";
    let blocks = analyze::route_blocks(text);
    assert!(enclosing_block(&blocks, 0).is_none(), "line 0 is top level");
    assert!(enclosing_block(&blocks, 2).is_some());
}

#[test]
fn calls_in_strings_and_comments_are_not_edges() {
    let text = "route {\n# route(COMMENTED);\n$var(x) = \"route(STRINGED)\";\nroute(REAL);\n}\n";
    let edges = call_edges(text);
    let names: Vec<&str> = edges.iter().map(|(_, c)| c.name.as_str()).collect();
    assert_eq!(names, vec!["REAL"], "decorative calls became edges");
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in ["", "route(", "route()", "}{", "route {\nroute(A);\n"] {
        let _ = call_edges(s);
        let _ = enclosing_block(&analyze::route_blocks(s), 0);
    }
}
