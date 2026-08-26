//! Edge cases of the manual-page parsers, found by probing them
//! rather than by reading them.
//!
//! Three of these were defects when this file was written: a section
//! with no prose became an entry that hovered blank, two sections of
//! the same name became two entries of which every lookup saw one,
//! and a capitalised heading became a name that could never match a
//! hover — the parser reporting success while the keyword silently
//! vanished. Each is the same shape as the `for each` heading: the
//! page says something slightly different from what the code assumed,
//! and nothing failed.

use opensips_lsp::catalog::{parse_core_routes_md, parse_core_statements_md};

fn names(v: Vec<opensips_lsp::catalog::Item>) -> Vec<String> {
    v.into_iter().map(|i| i.name).collect()
}

#[test]
fn a_section_with_no_prose_is_not_offered() {
    let got = names(parse_core_routes_md("## route\n\n## timer_route\n\nHas text.\n").unwrap());
    assert_eq!(
        got,
        vec!["timer_route"],
        "a heading with nothing under it is not documentation: hovering it shows a \
         header and an empty body, which reads as the server being broken"
    );
}

#[test]
fn duplicate_headings_yield_one_entry() {
    let got = parse_core_statements_md("## if\n\nFirst.\n\n## if\n\nSecond.\n").unwrap();
    let ifs: Vec<_> = got.iter().filter(|i| i.name == "if").collect();
    assert_eq!(
        ifs.len(),
        1,
        "every lookup is a `find`, so a second entry is invisible to hover and \
         duplicated in completion: {got:?}"
    );
    assert_eq!(ifs[0].doc.trim(), "First.", "the first section wins");
}

#[test]
fn an_alias_follows_the_entry_that_survived_dedup() {
    let got = parse_core_statements_md("## if\n\nFirst.\n\n## if\n\nSecond.\n").unwrap();
    let else_ = got
        .iter()
        .find(|i| i.name == "else")
        .expect("else is aliased");
    assert_eq!(
        else_.doc.trim(),
        "First.",
        "an alias must carry the surviving section's text, not a discarded one's — \
         comparing it against `if`'s own text instead would pass however dedup chose, \
         because both sides move together"
    );
}

#[test]
fn a_capitalised_heading_still_names_the_keyword() {
    let got = names(parse_core_statements_md("## If\n\nCapitalised.\n").unwrap());
    assert!(
        got.contains(&"if".to_string()),
        "keywords are lower case; a name differing by case never matches a hover \
         and the keyword disappears silently: {got:?}"
    );
    assert!(
        !got.contains(&"If".to_string()),
        "and the capitalised form must not be offered as well: {got:?}"
    );
}

#[test]
fn a_capitalised_prose_heading_still_maps_to_its_keyword() {
    let got = names(parse_core_statements_md("## For Each\n\nIterates.\n").unwrap());
    assert_eq!(
        got,
        vec!["for"],
        "the prose-heading table must match case-insensitively too, or a heading \
         upstream capitalises drops the statement entirely"
    );
}

#[test]
fn a_prose_heading_with_no_prose_is_dropped() {
    let got = names(parse_core_statements_md("## for each\n\n## if\n\nText.\n").unwrap());
    assert!(
        !got.contains(&"for".to_string()),
        "a mapped heading is not exempt from needing text: {got:?}"
    );
}

#[test]
fn a_heading_inside_a_fenced_block_opens_no_section() {
    let got = names(
        parse_core_routes_md("## route\n\nReal.\n\n```\n## startup_route\nbody\n```\n").unwrap(),
    );
    assert_eq!(
        got,
        vec!["route"],
        "a `##` inside an example is example text — reading it as a heading is how the \
         module harvester once lost a third of its parameters. The fenced section needs \
         a body of its own, or the empty-doc rule would drop it whatever the fence rule \
         did: {got:?}"
    );
}

#[test]
fn duplicate_route_headings_also_yield_one_entry() {
    let got = parse_core_routes_md("## route\n\nFirst.\n\n## route\n\nSecond.\n").unwrap();
    assert_eq!(
        got.len(),
        1,
        "the two manual parsers must agree: a route page repeating a heading is the \
         same defect as a statement page repeating one — one dead entry in completion \
         that hover can never reach: {got:?}"
    );
    assert_eq!(
        got[0].doc.trim(),
        "First.",
        "the first section wins here too"
    );
}

/// The level is part of the contract: these pages head their entries
/// at `##`, and a demotion upstream would drop every one of them.
/// Pinning it means that arrives as a failure rather than as silence.
#[test]
fn a_deeper_heading_is_not_a_section() {
    let got = names(parse_core_statements_md("### if\n\nToo deep.\n").unwrap());
    assert!(
        got.is_empty(),
        "entries are `##`; if upstream demotes them this must fail loudly rather \
         than harvest nothing and report success: {got:?}"
    );
}
