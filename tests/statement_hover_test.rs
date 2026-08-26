//! Hovering a control keyword says what it does.
//!
//! GIVEN `if`, `while`, `switch` and `for` are how a configuration is
//! controlled,
//! WHEN a reader hovers one, or reads it in the completion popup,
//! THEN they get its documentation.
//!
//! They got neither. The keywords have always been offered in
//! completion — with `detail: "keyword"` and no documentation at all —
//! and hover returned null, which reads as the feature being absent
//! rather than empty. Upstream documents them in
//! `docs/manual/Script-Statements.md`, a page the harvester did not
//! read.

mod common;

use opensips_lsp::catalog::{harvest_core, parse_core_statements_md};

const PAGE: &str = r#"# Statements

## if

The if statement chooses between two paths.

A second paragraph that is not the summary.

## while

Repeats while the condition holds.

## for each

Iterates over an indexed variable.
"#;

#[test]
fn a_statements_page_yields_one_entry_per_heading() {
    let got = parse_core_statements_md(PAGE).expect("the page parses");
    let names: Vec<&str> = got.iter().map(|s| s.name.as_str()).collect();
    // `for each` is prose, not a keyword: the keyword is `for`, and a
    // parser that took the heading verbatim would document something
    // no configuration can write.
    //
    // `else` follows because this fixture HAS an `if` section for it
    // to be explained under. `case` and `default` do not, because it
    // has no `switch` — an alias is carried from a section that
    // exists, never invented because the keyword exists.
    assert_eq!(names, vec!["if", "while", "for", "else"]);
    assert!(
        !names.contains(&"case") && !names.contains(&"default"),
        "aliases must not appear without the section explaining them: {names:?}"
    );

    let if_ = got.iter().find(|s| s.name == "if").unwrap();
    assert!(
        if_.doc.starts_with("The if statement chooses"),
        "first paragraph only: {:?}",
        if_.doc
    );
    assert!(
        !if_.doc.contains("not the summary"),
        "and it stops there: {:?}",
        if_.doc
    );
}

#[test]
fn adversarial_pages_do_not_panic() {
    for md in ["", "\0\0", "## \n\n", "## if", "prose with no headings\n"] {
        let _ = parse_core_statements_md(md);
    }
}

/// The real page documents the four statements that have sections.
#[test]
fn the_real_tree_documents_the_control_statements() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    let names: Vec<&str> = core.statements.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.len() >= 4,
        "only {} statement(s) harvested: {names:?}",
        names.len()
    );
    for kw in ["if", "switch", "while", "for"] {
        let it = core
            .statements
            .iter()
            .find(|s| s.name == kw)
            .unwrap_or_else(|| panic!("{kw} is not documented: {names:?}"));
        assert!(!it.doc.trim().is_empty(), "{kw} has an empty summary");
    }
}

/// Keywords documented inside another statement's section carry that
/// section's text, and say so.
///
/// `else` has no section of its own — it is explained under `if`, as
/// `case` and `default` are under `switch`. Answering nothing for them
/// would leave the commonest keyword in the language silent; inventing
/// text for them would be worse. Pointing at where they ARE explained
/// is the honest third option, and each alias is named rather than
/// guessed from proximity.
#[test]
fn keywords_explained_under_another_section_point_at_it() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    for (alias, parent) in [("else", "if"), ("case", "switch"), ("default", "switch")] {
        let it = core
            .statements
            .iter()
            .find(|s| s.name == alias)
            .unwrap_or_else(|| panic!("{alias} must be answerable"));
        assert!(
            it.detail.contains(parent),
            "{alias} must say it is documented under {parent}: {:?}",
            it.detail
        );
        let owner = core.statements.iter().find(|s| s.name == parent).unwrap();
        assert_eq!(
            it.doc, owner.doc,
            "{alias} carries {parent}'s text rather than invented text"
        );
    }
}

/// Hover answers for a control keyword.
#[test]
fn hovering_a_control_keyword_answers() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    for kw in ["if", "while", "switch", "for", "else"] {
        let doc = format!("route {{\n    {kw}\n}}\n");
        let got = opensips_lsp::logic::hover_markdown_at(&[], core, &doc, kw, 1, 4)
            .unwrap_or_else(|| panic!("{kw} must hover"));
        assert!(got.contains(kw), "the hover must name it: {got:?}");
        assert!(
            !got.trim().is_empty(),
            "{kw} hovered with empty text: {got:?}"
        );
    }
}

/// And the completion popup carries the same text, so the reader does
/// not have to hover to find out what a keyword is.
#[test]
fn keyword_completions_carry_their_documentation() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    let items = opensips_lsp::logic::completions_with_core(&[], core, "route {\n}\n", "    ");
    let mut checked = 0usize;
    for kw in ["if", "while", "switch", "for"] {
        let it = items
            .iter()
            .find(|i| i.label == kw)
            .unwrap_or_else(|| panic!("{kw} must be offered"));
        checked += 1;
        assert!(
            !it.doc.trim().is_empty(),
            "{kw} is offered with no documentation: {it:?}"
        );
    }
    assert_eq!(checked, 4, "control control: all four were examined");
}

/// Every keyword the completion offers must carry documentation.
///
/// `CORE_KEYWORDS` is a hand-written list and the documentation comes
/// from two harvested pages; nothing connected them. Every one of the
/// twelve was offered with empty text for as long as the list has
/// existed, and no test could tell. A keyword added to that list
/// tomorrow would be silent in exactly the same way.
///
/// The list is read from the source rather than restated here, so
/// this cannot drift from what is actually offered.
#[test]
fn every_offered_keyword_carries_documentation() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/logic.rs"))
        .expect("the source is readable");
    let block = src
        .split_once("const CORE_KEYWORDS: &[&str] = &[")
        .and_then(|(_, r)| r.split_once("];"))
        .map(|(b, _)| b)
        .expect("CORE_KEYWORDS exists");
    let keywords: Vec<String> = block
        .split(',')
        .filter_map(|t| {
            let t = t.trim().trim_matches('"');
            (!t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then(|| t.to_string())
        })
        .collect();

    // POSITIVE CONTROL: a scan that stopped matching would loop over
    // nothing and pass having checked no keyword at all.
    assert!(
        keywords.len() >= 10,
        "only {} keyword(s) scanned: {keywords:?}",
        keywords.len()
    );

    let core = &opensips_lsp::catalog::builtin_core().core;
    let silent: Vec<&String> = keywords
        .iter()
        .filter(|k| {
            let documented = |v: &[opensips_lsp::catalog::Item]| {
                v.iter().any(|i| &i.name == *k && !i.doc.trim().is_empty())
            };
            !documented(&core.statements) && !documented(&core.functions)
        })
        .collect();
    assert!(
        silent.is_empty(),
        "keyword(s) offered in completion with nothing explaining them: {silent:?}"
    );
}

/// No harvested statement is named something a configuration cannot
/// write.
///
/// The page heads its iteration statement `for each`, which is prose.
/// Carried verbatim it would document a keyword nobody can type, and
/// would never match a hover — failing silently, looking like the
/// statement simply had no documentation. That mapping is one entry
/// in a table today, and this holds the whole surface rather than
/// that one case.
#[test]
fn every_harvested_statement_is_a_writable_keyword() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    assert!(
        core.statements.len() >= 4,
        "control: {} statement(s) harvested",
        core.statements.len()
    );
    for s in &core.statements {
        assert!(
            !s.name.is_empty()
                && s.name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "{:?} is not a keyword a configuration can write",
            s.name
        );
    }
}

/// The async page names its statements in prose, beside sections that
/// are not statements at all.
///
/// `## async() statement` and `## launch() statement` carry the
/// keyword inside the heading, and `## Description` and
/// `## Limitations` sit alongside them — identifier-shaped, and not
/// keywords. A parser permissive enough to read the first pair would
/// harvest the second pair too, and offer `Description` as something a
/// configuration can write. So the headings that mean something are
/// named, and nothing else is taken.
#[test]
fn the_async_page_yields_only_its_two_statements() {
    let page = "# Async\n\n## Description\n\nProse.\n\n## async() statement\n\nDoes async things.\n\n## launch() statement\n\nDoes launch things.\n\n## Limitations\n\nMore prose.\n";
    let got = opensips_lsp::catalog::parse_core_async_md(page).expect("parses");
    let names: Vec<&str> = got.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["async", "launch"]);
    assert!(
        got.iter().all(|s| !s.doc.trim().is_empty()),
        "each must carry its text: {got:?}"
    );
}

#[test]
fn the_real_async_page_documents_async_and_launch() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = harvest_core(std::path::Path::new(&tree));
    for kw in ["async", "launch"] {
        let it = core
            .statements
            .iter()
            .find(|s| s.name == kw)
            .unwrap_or_else(|| panic!("{kw} must be documented"));
        assert!(!it.doc.trim().is_empty(), "{kw} has an empty summary");
    }
    // and the prose sections beside them must NOT have been taken
    for not_a_keyword in ["Description", "Limitations"] {
        assert!(
            !core.statements.iter().any(|s| s.name == not_a_keyword),
            "{not_a_keyword} is a section heading, not a statement"
        );
    }
}

/// Every offered keyword carries its text THROUGH the completion
/// path, not merely in the catalogue.
///
/// The gate above asks whether the catalogue documents each keyword.
/// That is a different question from whether the popup shows it: the
/// completion builder copies the text across, and a keyword can be
/// perfectly documented and still be offered blank if that copy is
/// dropped. `async` and `launch` were silent because the catalogue
/// lacked them; this asks the other half, so neither half can go
/// quiet alone.
#[test]
fn every_offered_keyword_reaches_the_popup_with_its_text() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/logic.rs"))
        .expect("the source is readable");
    let block = src
        .split_once("const CORE_KEYWORDS: &[&str] = &[")
        .and_then(|(_, r)| r.split_once("];"))
        .map(|(b, _)| b)
        .expect("CORE_KEYWORDS exists");
    let keywords: Vec<String> = block
        .split(',')
        .filter_map(|t| {
            let t = t.trim().trim_matches('"');
            (!t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                .then(|| t.to_string())
        })
        .collect();
    assert!(keywords.len() >= 10, "control: {keywords:?}");

    let core = &opensips_lsp::catalog::builtin_core().core;
    let items = opensips_lsp::logic::completions_with_core(&[], core, "route {\n}\n", "    ");

    let mut blank: Vec<String> = Vec::new();
    for k in &keywords {
        // a keyword may be offered more than once (keyword list and
        // core functions both carry `route`); ANY offering with text
        // is enough for the reader
        let offerings: Vec<_> = items.iter().filter(|i| &i.label == k).collect();
        assert!(!offerings.is_empty(), "{k} is not offered at all");
        if !offerings.iter().any(|i| !i.doc.trim().is_empty()) {
            blank.push(k.clone());
        }
    }
    assert!(
        blank.is_empty(),
        "keyword(s) reaching the popup with no text: {blank:?}"
    );
}

/// `async` and `launch` come from the async page, and from nowhere
/// else.
///
/// They were the two the derived gate caught, and the fix was to
/// harvest one more page. If that page stopped being read — renamed
/// upstream, dropped from the harvest — they would fall silent again
/// and the catalogue gate would be the only thing to notice, on a
/// tree that still happens to ship a vendored copy. This pins the
/// provenance: with the page they are present, without it they are
/// absent, and no other page supplies them.
#[test]
fn async_and_launch_come_from_the_async_page() {
    let dir = std::env::temp_dir().join(format!("oslsp-async-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let manual = dir.join("docs").join("manual");
    std::fs::create_dir_all(&manual).unwrap();

    // a tree with the statements page but NOT the async page
    std::fs::write(manual.join("Script-Statements.md"), "## if\n\nChooses.\n").unwrap();
    let without = harvest_core(&dir);
    assert!(
        without.statements.iter().any(|s| s.name == "if"),
        "control: the statements page must still be read"
    );
    for kw in ["async", "launch"] {
        assert!(
            !without.statements.iter().any(|s| s.name == kw),
            "{kw} appeared with no async page — it is coming from somewhere unintended"
        );
    }

    // add the async page: both appear
    std::fs::write(
        manual.join("Script-Async.md"),
        "## async() statement\n\nDoes async things.\n\n## launch() statement\n\nDoes launch things.\n",
    )
    .unwrap();
    let with = harvest_core(&dir);
    for kw in ["async", "launch"] {
        let it = with
            .statements
            .iter()
            .find(|s| s.name == kw)
            .unwrap_or_else(|| panic!("{kw} must come from the async page"));
        assert!(!it.doc.trim().is_empty(), "{kw} must carry its text");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
