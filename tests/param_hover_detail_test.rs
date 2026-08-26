//! A parameter's hover carries its default and its example.
//!
//! GIVEN upstream documents each core parameter as a description, a
//! `Default value is ...` line, and a worked example,
//! WHEN a reader hovers the parameter,
//! THEN they get all three.
//!
//! They got the first paragraph. `db_default_url` hovered "The
//! default DB URL used by modules when no per-module URL is
//! configured" and stopped — so the one thing a reader actually needs
//! from it, the URL FORMAT, was in the example block that the parse
//! dropped. Across the page that was 68 of 75 defaults and 96 of 99
//! examples, thrown away.
//!
//! The first-paragraph rule is right for a route type or a control
//! statement, where the summary is the answer. It is wrong for a
//! setting, where the answer is what to write.

mod common;

use opensips_lsp::catalog::parse_core_params_md;

const PAGE: &str = r#"# Core parameters

### db_default_url

The default DB URL used by modules when no per-module URL is configured.

Default value is `NULL` (not defined).

Example of usage:
```opensips

    db_default_url = "mysql://opensips:opensipsrw@localhost/opensips"

```

### children

Number of children.

Example of usage:
```opensips

    children = 16

```

### no_extras

Just a description, nothing else.

### db_max_async_connections

Maximum number of connections per backend.

Default value is `10`.

Individual backends are determined from DB URLs as follows:
```opensips

    [ scheme, user, pass, host, port, database ]

```

Example of usage:
```opensips

    db_max_async_connections = 220

```

### fenced_heading

A section whose example contains a comment at column zero.

Example of usage:
```opensips

# a comment, which markdown reads as a heading
fenced_heading = 1

```
"#;

fn param<'a>(v: &'a [opensips_lsp::catalog::Item], name: &str) -> &'a opensips_lsp::catalog::Item {
    v.iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("{name} missing"))
}

#[test]
fn the_hover_carries_the_worked_example() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let db = param(&got, "db_default_url");
    assert!(
        db.doc
            .contains("mysql://opensips:opensipsrw@localhost/opensips"),
        "the example is where the FORMAT is — a description of what the setting \
         means does not tell a reader what to type: {:?}",
        db.doc
    );
}

#[test]
fn the_hover_carries_the_default_value() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let db = param(&got, "db_default_url");
    assert!(
        db.doc.contains("NULL"),
        "the default decides whether you need to set it at all: {:?}",
        db.doc
    );
}

#[test]
fn the_description_is_still_first() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let db = param(&got, "db_default_url");
    assert!(
        db.doc.starts_with("The default DB URL"),
        "a hover opens with what the setting is, not with an example: {:?}",
        db.doc
    );
}

#[test]
fn a_parameter_with_no_default_still_gets_its_example() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let ch = param(&got, "children");
    assert!(ch.doc.contains("children = 16"), "{:?}", ch.doc);
    assert!(
        !ch.doc.contains("Default value"),
        "and no empty `Default value` heading invented for it: {:?}",
        ch.doc
    );
}

#[test]
fn a_parameter_with_neither_is_unchanged() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let n = param(&got, "no_extras");
    assert_eq!(n.doc.trim(), "Just a description, nothing else.");
}

#[test]
fn one_section_does_not_borrow_the_next_ones_example() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let db = param(&got, "db_default_url");
    assert!(
        !db.doc.contains("children = 16"),
        "the scan must stop at the next heading: {:?}",
        db.doc
    );
}

#[test]
fn adversarial_sections_do_not_panic() {
    for md in [
        "### x\n\nd\n\n```opensips\nunclosed\n",
        "### x\n\n```opensips\n```\n",
        "### x\n\nDefault value is\n",
        "",
    ] {
        let _ = parse_core_params_md(md);
    }
}

/// The real page, not a fixture: the reported case end to end.
#[test]
fn the_real_page_gives_db_default_url_its_format() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = opensips_lsp::catalog::harvest_core(std::path::Path::new(&tree));

    // POSITIVE CONTROL: an empty harvest must not read as one miss.
    assert!(core.params.len() > 50, "only {} params", core.params.len());
    let db = param(&core.params, "db_default_url");
    assert!(
        db.doc.contains("mysql://"),
        "the reported gap: hovering it must show the URL format: {:?}",
        db.doc
    );

    // and it is not one parameter that got lucky
    let with_example = core.params.iter().filter(|p| p.doc.contains("```")).count();
    assert!(
        with_example >= 90,
        "only {with_example} of {} parameters carry their example",
        core.params.len()
    );
}

/// Prose that follows the default line is elaboration, not the
/// summary — `db_max_async_connections` explains how backends are
/// identified after stating its default, and a hover that opens with
/// three paragraphs buries the one sentence that answers the
/// question.
#[test]
fn prose_after_the_default_does_not_join_the_description() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let it = param(&got, "db_max_async_connections");
    assert!(
        it.doc
            .starts_with("Maximum number of connections per backend."),
        "{:?}",
        it.doc
    );
    assert!(
        !it.doc.contains("Individual backends are determined"),
        "the elaboration belongs on the manual page, not in the popup: {:?}",
        it.doc
    );
}

/// A section with two fenced blocks is showing a form and then a
/// worked example. Concatenating them puts a schema and a value in
/// one code block, which reads as one broken example.
#[test]
fn only_the_first_fenced_block_is_taken() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let it = param(&got, "db_max_async_connections");
    assert!(
        it.doc.contains("[ scheme, user, pass"),
        "the first block is the one shown: {:?}",
        it.doc
    );
    assert!(
        !it.doc.contains("db_max_async_connections = 220"),
        "and the second is not glued to it: {:?}",
        it.doc
    );
}

/// A `#` at column zero inside an example is a CONFIGURATION COMMENT,
/// and markdown reads it as a heading. OpenSIPS examples are full of
/// them — 42 across the shipped manual — and a section that ends
/// there loses the rest of its example, which is the part a reader
/// hovers it for.
#[test]
fn a_comment_at_column_zero_inside_an_example_does_not_end_the_section() {
    let got = parse_core_params_md(PAGE).expect("parses");
    let it = param(&got, "fenced_heading");
    assert!(
        it.doc.contains("# a comment"),
        "the comment line belongs to the example: {:?}",
        it.doc
    );
    assert!(
        it.doc.contains("fenced_heading = 1"),
        "and so does everything after it — truncating there drops the line \
         that shows what to write: {:?}",
        it.doc
    );
}

/// A markdown list must survive as a list.
///
/// `socket` documents its twelve optional modifiers as a bullet per
/// modifier. Joining the description with spaces turns that into one
/// unbroken paragraph — twelve settings run together in a popup,
/// which is worse than not showing them.
#[test]
fn a_bullet_list_keeps_its_line_breaks() {
    const LIST: &str = r#"### socket

Sets the sockets to listen on.

The definition may accept several optional parameters:
* `use_workers n`: workers for this socket only.
* `reuse_port`: reuse the listening port as the source port.

Example of usage:
```opensips

    socket = udp:1.2.3.4:5060

```
"#;
    let got = parse_core_params_md(LIST).expect("parses");
    let it = param(&got, "socket");
    assert!(
        it.doc.contains("\n* `use_workers n`"),
        "each bullet needs its own line or markdown renders one paragraph: {:?}",
        it.doc
    );
    assert!(
        it.doc.contains("\n* `reuse_port`"),
        "every bullet, not just the first: {:?}",
        it.doc
    );
    // POSITIVE CONTROL: ordinary prose still flows, so this is not
    // just "every line got a break"
    assert!(
        it.doc
            .starts_with("Sets the sockets to listen on. The definition may accept"),
        "non-list prose must still join into a paragraph: {:?}",
        it.doc
    );
}
