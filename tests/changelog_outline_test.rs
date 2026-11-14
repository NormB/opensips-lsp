//! Format gate: every release section of client/CHANGELOG.md is an
//! outline. The marketplace renders the file as the "Changes" tab, and
//! its reader is deciding whether to upgrade and what to do after.
//!
//! The rule is stated in CONTRIBUTING.md under "Changelog entries".
//! Inside a release section (a `## ` heading and what follows it):
//!
//! - entries sit under a `### ` category heading from [`CATEGORIES`];
//! - an entry is a top-level bullet that is one bold lead of at most
//!   [`LEAD_WORDS`] words and nothing else;
//! - under it are 1 to [`MAX_SUBS`] sub-bullets, each starting with a
//!   label from [`LABELS`] and at most [`SUB_WORDS`] words long, the
//!   label counted, and one of them is `Effect:` or `Was:`; or, in
//!   their place, one table of at most [`TABLE_ROWS`] rows;
//! - nothing else: no paragraph, no third level of nesting, and no line
//!   wider than [`WRAP`] columns.
//!
//! A link counts by its text, for words and for width: its target is
//! not prose and cannot be wrapped.

/// The category headings a release section may carry.
const CATEGORIES: [&str; 6] = [
    "Added",
    "Changed",
    "Fixed",
    "Security",
    "Removed",
    "Deprecated",
];
/// The labels a sub-bullet may start with.
const LABELS: [&str; 5] = ["Effect", "Was", "Do", "Limit", "Docs"];
/// The most words a lead may have.
const LEAD_WORDS: usize = 15;
/// The most words a sub-bullet may have, its label counted.
const SUB_WORDS: usize = 25;
/// The most sub-bullets an entry may have.
const MAX_SUBS: usize = 5;
/// The most rows a table that replaces the sub-bullets may have.
const TABLE_ROWS: usize = 6;
/// The widest line an entry may have, in characters, link targets left out.
const WRAP: usize = 80;

/// One rule of the outline format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Text in a release section before its first entry.
    StrayText,
    /// A heading inside a release section that is not a category.
    Heading,
    /// An entry above the section's first category heading.
    NoCategory,
    /// A top-level bullet that is not exactly one bold lead.
    LeadNotBold,
    /// A lead of more than [`LEAD_WORDS`] words.
    LeadWords,
    /// No sub-bullet, or more than [`MAX_SUBS`].
    SubCount,
    /// A sub-bullet that does not start with a label from [`LABELS`].
    Label,
    /// A sub-bullet of more than [`SUB_WORDS`] words.
    SubWords,
    /// An entry with neither `Effect:` nor `Was:`.
    NoEffect,
    /// Text under an entry that continues neither its lead nor a sub-bullet.
    Paragraph,
    /// A bullet nested under a sub-bullet, or not indented by two spaces.
    ThirdLevel,
    /// A line wider than [`WRAP`] columns.
    Wrap,
    /// A table of more than [`TABLE_ROWS`] rows, or beside sub-bullets.
    Table,
}

impl Rule {
    /// The rule as the gate prints it.
    fn text(self) -> String {
        match self {
            Self::StrayText => "text before the section's first entry; a section holds category \
                                headings and entries only"
                .to_string(),
            Self::Heading => {
                format!("a heading inside a release section must be `### ` and one of {CATEGORIES:?}")
            }
            Self::NoCategory => "the entry is above the section's first `### ` category heading".to_string(),
            Self::LeadNotBold => {
                "the top-level bullet must be one bold lead (`- **Lead.**`) and nothing else".to_string()
            }
            Self::LeadWords => format!("the lead has more than {LEAD_WORDS} words"),
            Self::SubCount => format!("an entry has 1 to {MAX_SUBS} sub-bullets"),
            Self::Label => format!("a sub-bullet starts with one of {LABELS:?}, a colon and its text"),
            Self::SubWords => format!("a sub-bullet has more than {SUB_WORDS} words, its label counted"),
            Self::NoEffect => "an entry has an `Effect:` or a `Was:` sub-bullet".to_string(),
            Self::Paragraph => {
                "a paragraph under an entry; the text belongs in a labeled sub-bullet or in the docs"
                    .to_string()
            }
            Self::ThirdLevel => {
                "a bullet under a sub-bullet, or a sub-bullet not indented by two spaces".to_string()
            }
            Self::Wrap => format!("a line is wider than {WRAP} columns (link targets not counted)"),
            Self::Table => {
                format!("a table replaces the sub-bullets and has 1 to {TABLE_ROWS} rows under its header")
            }
        }
    }
}

/// One place where the changelog breaks a rule.
#[derive(Debug)]
struct Breach {
    /// The release heading, without its `## `.
    section: String,
    /// The entry's lead, or the offending line when no entry is open.
    lead: String,
    /// The 1-based line the breach is reported at.
    line: usize,
    /// The rule broken.
    rule: Rule,
}

/// An entry being read: its lead, its sub-bullets and its table.
#[derive(Debug)]
struct Entry {
    /// The line of the top-level bullet.
    line: usize,
    /// The bullet's text, continuation lines joined by a space.
    lead: String,
    /// Each sub-bullet's line and text, continuation lines joined.
    subs: Vec<(usize, String)>,
    /// How many table lines sit under the entry, header and rule included.
    table_lines: usize,
}

/// `text` with every link target removed: `[page](target)` becomes `[page]`.
fn without_link_targets(text: &str) -> String {
    let mut kept = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        kept.push_str(&rest[..=at]);
        let target = &rest[at + 2..];
        rest = target.find(')').map_or("", |end| &target[end + 1..]);
    }
    kept.push_str(rest);
    kept
}

/// How many words `text` has, a link counted by its text.
fn word_count(text: &str) -> usize {
    without_link_targets(text).split_whitespace().count()
}

/// The breaches of one finished entry, appended to `found`.
fn close_entry(entry: Option<Entry>, section: &str, found: &mut Vec<Breach>) {
    let Some(entry) = entry else { return };
    let mut flag = |line: usize, rule: Rule| {
        found.push(Breach {
            section: section.to_string(),
            lead: entry.lead.clone(),
            line,
            rule,
        });
    };
    let bold = entry
        .lead
        .strip_prefix("**")
        .and_then(|t| t.strip_suffix("**"))
        .filter(|t| !t.trim().is_empty() && !t.contains("**"));
    match bold {
        None => flag(entry.line, Rule::LeadNotBold),
        Some(lead) if word_count(lead) > LEAD_WORDS => flag(entry.line, Rule::LeadWords),
        Some(_) => {}
    }
    if entry.table_lines > 0 {
        let rows = entry.table_lines.saturating_sub(2);
        if !entry.subs.is_empty() || rows == 0 || rows > TABLE_ROWS {
            flag(entry.line, Rule::Table);
        }
        return;
    }
    if entry.subs.is_empty() || entry.subs.len() > MAX_SUBS {
        flag(entry.line, Rule::SubCount);
    }
    let mut effect = false;
    for (line, sub) in &entry.subs {
        match sub.split_once(": ") {
            Some((label, text)) if LABELS.contains(&label) && !text.trim().is_empty() => {
                effect |= label == "Effect" || label == "Was";
            }
            _ => flag(*line, Rule::Label),
        }
        if word_count(sub) > SUB_WORDS {
            flag(*line, Rule::SubWords);
        }
    }
    if !entry.subs.is_empty() && !effect {
        flag(entry.line, Rule::NoEffect);
    }
}

/// Every breach of the outline format in `changelog`: a line's own
/// breaches as the line is read, an entry's when the entry ends.
fn outline_breaches(changelog: &str) -> Vec<Breach> {
    let mut found = Vec::new();
    let mut section: Option<String> = None;
    let mut category = false;
    let mut entry: Option<Entry> = None;
    let mut after_blank = false;
    for (index, raw) in changelog.lines().enumerate() {
        let line = index + 1;
        if let Some(name) = raw.strip_prefix("## ") {
            close_entry(
                entry.take(),
                section.as_deref().unwrap_or_default(),
                &mut found,
            );
            section = Some(name.trim().to_string());
            category = false;
            after_blank = false;
            continue;
        }
        let Some(name) = section.as_deref() else {
            continue;
        };
        let text = raw.trim();
        if text.is_empty() {
            after_blank = true;
            continue;
        }
        let blank_above = std::mem::replace(&mut after_blank, false);
        let indent = raw.chars().take_while(|c| *c == ' ').count();
        let lead_now = entry
            .as_ref()
            .map_or_else(|| text.to_string(), |e| e.lead.clone());
        let mut flag = |rule: Rule, lead: &str| {
            found.push(Breach {
                section: name.to_string(),
                lead: lead.to_string(),
                line,
                rule,
            });
        };
        if !text.starts_with('|') && without_link_targets(raw).chars().count() > WRAP {
            flag(Rule::Wrap, &lead_now);
        }
        if raw.starts_with('#') {
            if raw
                .strip_prefix("### ")
                .is_some_and(|c| CATEGORIES.contains(&c.trim()))
            {
                category = true;
            } else {
                flag(Rule::Heading, text);
            }
            close_entry(entry.take(), name, &mut found);
            continue;
        }
        if let Some(lead) = raw.strip_prefix("- ") {
            if !category {
                flag(Rule::NoCategory, lead.trim());
            }
            close_entry(entry.take(), name, &mut found);
            entry = Some(Entry {
                line,
                lead: lead.trim().to_string(),
                subs: Vec::new(),
                table_lines: 0,
            });
            continue;
        }
        let Some(open) = entry.as_mut() else {
            flag(Rule::StrayText, text);
            continue;
        };
        if let Some(sub) = text.strip_prefix("- ") {
            if indent == 2 {
                open.subs.push((line, sub.trim().to_string()));
            } else {
                flag(Rule::ThirdLevel, &lead_now);
            }
            continue;
        }
        if text.starts_with('|') && indent == 2 {
            open.table_lines += 1;
            continue;
        }
        let continued = match open.subs.last_mut() {
            _ if blank_above || open.table_lines > 0 => None,
            Some((_, sub)) if indent == 4 => Some(sub),
            None if indent == 2 => Some(&mut open.lead),
            _ => None,
        };
        match continued {
            Some(joined) => {
                joined.push(' ');
                joined.push_str(text);
            }
            None => flag(Rule::Paragraph, &lead_now),
        }
    }
    close_entry(
        entry.take(),
        section.as_deref().unwrap_or_default(),
        &mut found,
    );
    found
}

/// The rules `changelog` breaks, in the order the gate reports them.
fn rules_of(changelog: &str) -> Vec<Rule> {
    outline_breaches(changelog).iter().map(|b| b.rule).collect()
}

/// A changelog with one release section, one category heading and `body`.
fn one_section(body: &str) -> String {
    format!("# Changelog\n\nAll notable changes.\n\n## [1.2.3] — 2026-01-02\n\n### Fixed\n\n{body}")
}

/// `count` words, separated by single spaces.
fn words(count: usize) -> String {
    vec!["a"; count].join(" ")
}

/// One test per case: the changelog text and the rules it must break.
macro_rules! outline_cases {
    ($($name:ident: $text:expr => [$($rule:ident),*];)+) => {$(
        #[test]
        fn $name() {
            let text: String = $text;
            let expected: Vec<Rule> = Vec::from([$(Rule::$rule),*]);
            assert_eq!(rules_of(&text), expected, "the rules broken by:\n{text}");
        }
    )+};
}

// Text the gate accepts.
outline_cases! {
    accepts_a_lead_with_an_effect:
        one_section("- **The outline folds.**\n  - Effect: a route folds.\n") => [];
    accepts_a_fix_with_was_alone:
        one_section("- **Rename keeps quotes.**\n  - Was: the quotes were dropped.\n") => [];
    accepts_all_five_labels:
        one_section(
            "- **Rename keeps quotes.**\n  - Was: dropped.\n  - Effect: kept.\n  - Do: nothing.\n  \
             - Limit: routes only.\n  - Docs: [Features](https://example.com/f).\n",
        ) => [];
    accepts_a_lead_of_fifteen_words:
        one_section(&format!("- **{}**\n  - Effect: one.\n", words(15))) => [];
    accepts_a_sub_bullet_of_twenty_five_words:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n", words(24))) => [];
    accepts_a_wrapped_lead:
        one_section("- **A lead that runs\n  onto a second line.**\n  - Effect: one.\n") => [];
    accepts_a_wrapped_sub_bullet:
        one_section("- **Lead.**\n  - Effect: a sentence that runs\n    onto a second line.\n") => [];
    accepts_two_entries:
        one_section("- **One.**\n  - Effect: a.\n- **Two.**\n  - Was: b.\n") => [];
    accepts_a_blank_line_between_entries:
        one_section("- **One.**\n  - Effect: a.\n\n- **Two.**\n  - Was: b.\n") => [];
    accepts_code_in_a_lead:
        one_section("- **`include_file \"` completes paths.**\n  - Effect: paths complete.\n") => [];
    accepts_a_second_category:
        one_section("- **One.**\n  - Was: a.\n\n### Security\n\n- **Two.**\n  - Effect: b.\n") => [];
    accepts_every_category:
        format!(
            "## [1.0.0] — 2026-01-02\n\n{}",
            CATEGORIES.map(|c| format!("### {c}\n\n- **Lead.**\n  - Effect: one.\n\n")).concat()
        ) => [];
    accepts_a_table_of_six_rows:
        one_section(&format!(
            "- **The limits changed.**\n  | Limit | Value |\n  | --- | --- |\n{}",
            "  | files | 500 |\n".repeat(6)
        )) => [];
    accepts_a_docs_link_whose_target_passes_the_wrap:
        one_section(&format!(
            "- **Lead.**\n  - Effect: one.\n  - Docs: [Features](https://example.com/{}).\n",
            "a".repeat(90)
        )) => [];
    accepts_a_line_of_exactly_eighty_columns:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n", "a".repeat(68))) => [];
}

// Text the gate refuses.
outline_cases! {
    refuses_a_bullet_that_is_not_bold:
        one_section("- The outline folds.\n  - Effect: a route folds.\n") => [LeadNotBold];
    refuses_prose_after_the_lead:
        one_section("- **The outline folds.** It did not before.\n  - Effect: folds.\n") => [LeadNotBold];
    refuses_two_bold_runs_in_a_lead:
        one_section("- **One** and **two.**\n  - Effect: folds.\n") => [LeadNotBold];
    refuses_an_empty_bold_lead:
        one_section("- ****\n  - Effect: folds.\n") => [LeadNotBold];
    refuses_a_lead_of_sixteen_words:
        one_section(&format!("- **{}**\n  - Effect: one.\n", words(16))) => [LeadWords];
    refuses_an_entry_without_sub_bullets:
        one_section("- **The outline folds.**\n") => [SubCount];
    refuses_six_sub_bullets:
        one_section(&format!("- **Lead.**\n{}", "  - Effect: one.\n".repeat(6))) => [SubCount];
    refuses_a_sub_bullet_without_a_label:
        one_section("- **Lead.**\n  - Effect: one.\n  - It folds now.\n") => [Label];
    refuses_an_unknown_label:
        one_section("- **Lead.**\n  - Effect: one.\n  - Note: two.\n") => [Label];
    refuses_a_label_with_no_text:
        one_section("- **Lead.**\n  - Was: one.\n  - Do:\n") => [Label];
    refuses_a_sub_bullet_of_twenty_six_words:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n", words(25))) => [SubWords];
    refuses_an_entry_with_neither_effect_nor_was:
        one_section("- **Lead.**\n  - Do: set `x`.\n  - Limit: one.\n") => [NoEffect];
    refuses_a_paragraph_under_an_entry:
        one_section("- **Lead.**\n  - Effect: one.\n\n  More about it.\n") => [Paragraph];
    refuses_a_paragraph_after_the_entries:
        one_section("- **Lead.**\n  - Effect: one.\n\nNothing else changed.\n") => [Paragraph];
    refuses_a_third_level:
        one_section("- **Lead.**\n  - Effect: one.\n    - deeper.\n") => [ThirdLevel];
    refuses_a_summary_paragraph_under_the_release_heading:
        "## [1.2.3] — 2026-01-02\n\n**A summary.**\n\n### Fixed\n\n- **Lead.**\n  - Was: one.\n"
            .to_string() => [StrayText];
    refuses_an_entry_above_the_first_category:
        "## [1.2.3] — 2026-01-02\n\n- **Lead.**\n  - Effect: one.\n".to_string() => [NoCategory];
    refuses_an_unknown_category:
        one_section("- **Lead.**\n  - Effect: one.\n\n### Internal\n") => [Heading];
    refuses_a_fourth_level_heading:
        one_section("- **Lead.**\n  - Effect: one.\n\n#### Fixed\n") => [Heading];
    refuses_a_line_of_eighty_one_columns:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n", "a".repeat(69))) => [Wrap];
    refuses_a_table_of_seven_rows:
        one_section(&format!(
            "- **The limits changed.**\n  | Limit | Value |\n  | --- | --- |\n{}",
            "  | files | 500 |\n".repeat(7)
        )) => [Table];
    refuses_a_table_beside_sub_bullets:
        one_section(
            "- **The limits changed.**\n  - Effect: one.\n  | Limit | Value |\n  | --- | --- |\n  \
             | files | 500 |\n",
        ) => [Table];
    refuses_a_table_with_no_rows:
        one_section("- **The limits changed.**\n  | Limit | Value |\n  | --- | --- |\n") => [Table];
}

// Edges of the rules.
outline_cases! {
    edge_an_empty_changelog_breaks_nothing:
        String::new() => [];
    edge_text_before_the_first_release_is_free:
        "# Changelog\n\nAny prose here.\n\n- a plain bullet\n    - nested\n".to_string() => [];
    edge_a_section_without_entries_breaks_nothing:
        "## [1.2.3] — 2026-01-02\n\n### Fixed\n".to_string() => [];
    edge_an_unreleased_section_is_checked:
        "## [Unreleased]\n\n### Added\n\n- plain\n  - Effect: one.\n".to_string() => [LeadNotBold];
    edge_the_last_entry_is_checked_without_a_final_newline:
        one_section("- **Lead.**") => [SubCount];
    edge_a_category_heading_closes_the_entry_above_it:
        one_section("- **Lead.**\n\n### Added\n\n- **Two.**\n  - Effect: one.\n") => [SubCount];
    edge_a_release_heading_closes_the_entry_above_it:
        one_section("- **Lead.**\n\n## [1.2.2] — 2026-01-01\n") => [SubCount];
    edge_crlf_line_endings_are_read:
        one_section("- **Lead.**\r\n  - Effect: one.\r\n") => [];
    edge_the_label_counts_as_a_word:
        one_section(&format!("- **Lead.**\n  - Docs: {}\n", words(25))) => [SubWords, NoEffect];
    edge_a_link_target_is_not_counted_as_words:
        one_section(&format!(
            "- **Lead.**\n  - Effect: {} [page](https://example.com/p \"a title\").\n",
            words(23)
        )) => [];
    edge_link_text_is_counted_as_words:
        one_section(&format!(
            "- **Lead.**\n  - Effect: {} [two words](https://example.com/p).\n",
            words(23)
        )) => [SubWords];
    edge_width_counts_characters_not_bytes:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n", "—".repeat(68))) => [];
    edge_width_counts_link_text:
        one_section(&format!("- **Lead.**\n  - Effect: [{}](https://example.com).\n", "a".repeat(70)))
            => [Wrap];
    edge_a_table_row_is_not_wrapped:
        one_section(&format!(
            "- **Lead.**\n  | Limit | Value |\n  | --- | --- |\n  | {} | 1 |\n",
            "a".repeat(90)
        )) => [];
    edge_a_continuation_after_a_blank_line_is_a_paragraph:
        one_section("- **Lead.**\n  - Effect: one\n\n    and more.\n") => [Paragraph];
    edge_a_sub_bullet_continued_at_the_lead_indent_is_a_paragraph:
        one_section("- **Lead.**\n  - Effect: one\n  and more.\n") => [Paragraph];
    edge_a_lead_continued_at_the_sub_bullet_indent_is_a_paragraph:
        one_section("- **Lead\n    more.**\n  - Effect: one.\n") => [Paragraph, LeadNotBold];
    edge_a_sub_bullet_indented_three_spaces:
        one_section("- **Lead.**\n  - Effect: one.\n   - Do: two.\n") => [ThirdLevel];
    edge_a_lowercase_label_is_not_a_label:
        one_section("- **Lead.**\n  - effect: one.\n") => [Label, NoEffect];
    edge_a_space_before_the_colon_is_not_a_label:
        one_section("- **Lead.**\n  - Effect : one.\n") => [Label, NoEffect];
    edge_a_wrapped_lead_is_counted_whole:
        one_section(&format!("- **{}\n  {}**\n  - Effect: one.\n", words(8), words(8))) => [LeadWords];
    edge_a_wrapped_sub_bullet_is_counted_whole:
        one_section(&format!("- **Lead.**\n  - Effect: {}\n    {}\n", words(12), words(13)))
            => [SubWords];
    edge_a_sub_bullet_before_any_entry_is_stray_text:
        one_section("  - Effect: one.\n") => [StrayText];
    edge_each_breach_of_one_entry_is_reported:
        one_section("- plain\n  - Note: one.\n") => [LeadNotBold, Label, NoEffect];
}

/// A breach names the release section, the lead and the line, so the
/// reader of a failed gate can find the entry.
#[test]
fn a_breach_names_its_section_lead_and_line() {
    let text = "## [2.0.0] — 2026-02-01\n\n### Added\n\n- **Good.**\n  - Effect: one.\n\n## [1.9.0] — \
                2026-01-01\n\n### Fixed\n\n- **Bad lead.**\n  - Note: one.\n";
    let breaches = outline_breaches(text);
    assert_eq!(
        breaches.len(),
        2,
        "one unknown label and no effect: {breaches:?}"
    );
    assert_eq!(
        breaches[0].section, "[1.9.0] — 2026-01-01",
        "the section of {:?}",
        breaches[0]
    );
    assert_eq!(
        breaches[0].lead, "**Bad lead.**",
        "the lead of {:?}",
        breaches[0]
    );
    assert_eq!(
        breaches[0].line, 13,
        "the sub-bullet's line: {:?}",
        breaches[0]
    );
    assert_eq!(breaches[1].line, 12, "the entry's line: {:?}", breaches[1]);
}

/// Every rule prints text of its own, so a failed gate says which rule.
#[test]
fn every_rule_has_its_own_text() {
    let rules = [
        Rule::StrayText,
        Rule::Heading,
        Rule::NoCategory,
        Rule::LeadNotBold,
        Rule::LeadWords,
        Rule::SubCount,
        Rule::Label,
        Rule::SubWords,
        Rule::NoEffect,
        Rule::Paragraph,
        Rule::ThirdLevel,
        Rule::Wrap,
        Rule::Table,
    ];
    let mut texts: Vec<String> = rules.iter().map(|r| r.text()).collect();
    texts.sort();
    texts.dedup();
    assert_eq!(
        texts.len(),
        rules.len(),
        "two rules print the same text: {texts:?}"
    );
}

/// The gate itself: every release section of the shipped changelog.
#[test]
fn every_release_section_is_an_outline() {
    let root = env!("CARGO_MANIFEST_DIR");
    let changelog = std::fs::read_to_string(format!("{root}/client/CHANGELOG.md"))
        .expect("client/CHANGELOG.md must exist");
    // POSITIVE CONTROL: a file with no entries would pass every rule.
    let entries = changelog.lines().filter(|l| l.starts_with("- **")).count();
    assert!(
        entries >= 50,
        "expected a history of entries, found {entries}"
    );
    let report: Vec<String> = outline_breaches(&changelog)
        .iter()
        .map(|b| {
            format!(
                "line {}: ## {}: {}: {}",
                b.line,
                b.section,
                b.lead,
                b.rule.text()
            )
        })
        .collect();
    assert!(
        report.is_empty(),
        "client/CHANGELOG.md breaks the outline format (CONTRIBUTING.md, \"Changelog entries\") in {} \
         place(s):\n{}",
        report.len(),
        report.join("\n")
    );
}

/// The contributor page states the rule the gate holds: every label,
/// every category and both word limits.
#[test]
fn the_contributor_page_states_the_outline_rule() {
    let root = env!("CARGO_MANIFEST_DIR");
    let page = std::fs::read_to_string(format!("{root}/CONTRIBUTING.md")).expect("CONTRIBUTING.md");
    assert!(
        page.contains("## Changelog entries"),
        "CONTRIBUTING.md has no \"Changelog entries\" section"
    );
    for label in LABELS {
        assert!(
            page.contains(&format!("`{label}:`")),
            "CONTRIBUTING.md does not name the label `{label}:`"
        );
    }
    for category in CATEGORIES {
        assert!(
            page.contains(&format!("`### {category}`")),
            "CONTRIBUTING.md does not name the category `### {category}`"
        );
    }
    for limit in [LEAD_WORDS, SUB_WORDS, WRAP] {
        assert!(
            page.contains(&format!(" {limit} ")),
            "CONTRIBUTING.md does not state the limit {limit}"
        );
    }
}
