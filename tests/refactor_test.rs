//! Extract-route and duplicate-loadmodule removal.
//!
//! Both refuse more than they accept, on purpose. A refactoring that
//! silently changes what a config does is worse than one that declines.

mod common;
use opensips_lsp::logic::{duplicate_loadmodules, extract_route};

const CFG: &str = "\
loadmodule \"tm.so\"
route {
    xlog(\"one\");
    xlog(\"two\");
    exit;
}
route[EXTRACTED] {
    exit;
}
";

#[test]
fn a_selection_becomes_a_new_route_and_a_call_in_its_place() {
    // lines 2..3 are the two xlog calls
    let plan = extract_route(CFG, 2, 3).expect("extractable");
    assert_eq!(
        plan.name, "EXTRACTED_2",
        "the obvious name is taken, so the next free one is used"
    );
    assert_eq!(plan.start_line, 2);
    assert_eq!(plan.end_line, 3);
    assert_eq!(
        plan.call_line, "    route(EXTRACTED_2);",
        "the call keeps the indentation of what it replaces"
    );
    assert!(
        plan.block.starts_with("route[EXTRACTED_2] {"),
        "{:?}",
        plan.block
    );
    assert!(plan.block.contains("xlog(\"one\");"));
    assert!(plan.block.contains("xlog(\"two\");"));
    // the new block goes after the enclosing one, never inside it
    assert_eq!(plan.insert_line, 6);
}

#[test]
fn a_selection_outside_any_route_block_is_not_extractable() {
    assert!(extract_route(CFG, 0, 0).is_none(), "loadmodule line");
}

#[test]
fn a_selection_covering_the_blocks_own_braces_is_refused() {
    // line 1 opens the block, line 5 closes it
    assert!(extract_route(CFG, 1, 3).is_none(), "includes the opener");
    assert!(extract_route(CFG, 3, 5).is_none(), "includes the closer");
}

#[test]
fn an_unbalanced_selection_is_refused() {
    let text = "route {\n    if (1) {\n        exit;\n    }\n}\n";
    // just the `if (1) {` line: taking it alone would strand its brace
    assert!(extract_route(text, 1, 1).is_none());
    // the whole balanced if-block is fine
    assert!(extract_route(text, 1, 3).is_some());
}

/// `return` leaves the route it is written in.  Move it into a new
/// route and it returns to the caller instead — the flow after the
/// extracted call now runs when it did not before.  That is a
/// behaviour change no editor should make silently.
#[test]
fn a_selection_containing_return_is_refused() {
    let text = "route {\n    xlog(\"x\");\n    return;\n    exit;\n}\n";
    assert!(extract_route(text, 1, 2).is_none(), "contains return");
    assert!(
        extract_route(text, 1, 1).is_some(),
        "the same selection without return is fine"
    );
    // a `return` inside a string or comment is not a return
    let text = "route {\n    xlog(\"return\");\n    # return\n    exit;\n}\n";
    assert!(extract_route(text, 1, 2).is_some(), "decorative returns");
}

#[test]
fn a_blank_selection_is_refused() {
    let text = "route {\n\n\n    exit;\n}\n";
    assert!(extract_route(text, 1, 2).is_none());
}

#[test]
fn duplicate_loadmodules_are_found_after_the_first() {
    let text = "loadmodule \"tm.so\"\nloadmodule \"sl.so\"\nloadmodule \"tm.so\"\n\
                # loadmodule \"tm.so\"\nloadmodule \"sl.so\"\nroute { exit; }\n";
    assert_eq!(
        duplicate_loadmodules(text),
        vec![2, 4],
        "the first occurrence stays; a commented one is not a load"
    );
}

#[test]
fn a_document_without_duplicates_reports_none() {
    assert!(duplicate_loadmodules(CFG).is_empty());
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in ["", "route {", "}", "route {\n\n}", "loadmodule"] {
        let _ = extract_route(s, 0, 0);
        let _ = extract_route(s, 0, 99);
        let _ = duplicate_loadmodules(s);
    }
}
