//! Formatter contract.
//!
//! The formatter is deliberately line-preserving: it rewrites the
//! leading and trailing whitespace of a line and nothing else.  It
//! never joins, splits or reorders lines, and it never touches a byte
//! inside a string literal or a comment.  That restraint is what makes
//! the safety proof cheap — a config that reformatted into a
//! different parse would be worse than no formatter at all.

use opensips_lsp::format::{Options, format, format_lines};

fn tabs() -> Options {
    Options {
        insert_spaces: false,
        tab_size: 4,
    }
}
fn spaces(n: u32) -> Options {
    Options {
        insert_spaces: true,
        tab_size: n,
    }
}

/// Every line, stripped of leading and trailing whitespace.  Two
/// documents agreeing on this agree on everything the parser can see
/// except indentation — the formatter must never change it.
fn skeleton(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().to_string()).collect()
}

#[test]
fn indents_route_bodies_by_brace_depth() {
    let src = "route {\nxlog(\"hi\");\nif (1) {\nexit;\n}\n}\n";
    assert_eq!(
        format(src, &tabs()),
        "route {\n\txlog(\"hi\");\n\tif (1) {\n\t\texit;\n\t}\n}\n"
    );
}

#[test]
fn honours_the_clients_indent_settings() {
    let src = "route {\nexit;\n}\n";
    assert_eq!(format(src, &spaces(2)), "route {\n  exit;\n}\n");
    assert_eq!(format(src, &spaces(4)), "route {\n    exit;\n}\n");
    assert_eq!(format(src, &tabs()), "route {\n\texit;\n}\n");
}

#[test]
fn a_closing_brace_dedents_itself() {
    let src = "route {\nif (1) {\nexit;\n}\n}\n";
    let out = format(src, &spaces(2));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[3], "  }", "inner close dedents to its opener");
    assert_eq!(lines[4], "}", "outer close returns to column 0");
}

#[test]
fn strips_trailing_whitespace() {
    let src = "route {   \n\texit;\t\n}  \n";
    let out = format(src, &tabs());
    assert!(
        out.lines().all(|l| l == l.trim_end()),
        "trailing whitespace survived: {out:?}"
    );
}

#[test]
fn braces_inside_strings_and_comments_do_not_move_the_depth() {
    // a `{` that is not code must not indent everything after it
    let src = "route {\nxlog(\"a { b\");\n# a } here\nexit;\n}\n";
    let out = format(src, &spaces(2));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[1], "  xlog(\"a { b\");");
    assert_eq!(lines[2], "  # a } here");
    assert_eq!(lines[3], "  exit;", "depth survived the decorative braces");
    assert_eq!(lines[4], "}");
}

#[test]
fn string_interiors_are_never_reindented() {
    // a literal newline inside a string: the second line belongs to
    // the string's value, so its leading whitespace is data
    let src = "route {\nxlog(\"first\n    second\");\nexit;\n}\n";
    let out = format(src, &tabs());
    assert!(
        out.contains("\n    second\");"),
        "string interior was reindented: {out:?}"
    );
}

#[test]
fn block_comment_interiors_keep_their_own_alignment() {
    let src = "route {\n/* aligned\n   like this\n   and this */\nexit;\n}\n";
    let out = format(src, &spaces(2));
    assert!(
        out.contains("\n   like this\n   and this */"),
        "comment body was reindented: {out:?}"
    );
}

#[test]
fn is_idempotent() {
    let src = "route {\n   xlog(\"x\");\n\t\tif (1) {\nexit;\n   }\n}\n";
    for opts in [tabs(), spaces(2), spaces(8)] {
        let once = format(src, &opts);
        let twice = format(&once, &opts);
        assert_eq!(once, twice, "second pass changed the document");
    }
}

#[test]
fn changes_nothing_but_leading_and_trailing_whitespace() {
    let src = "# comment\nloadmodule \"tm.so\"\nmodparam(\"tm\", \"fr_timeout\", 3)\n\
               route {\n  if ($rU == \"x\") {\n t_relay();\n}\n}\n";
    for opts in [tabs(), spaces(2)] {
        let out = format(src, &opts);
        assert_eq!(
            skeleton(&out),
            skeleton(src),
            "the formatter altered document content, not just indentation"
        );
    }
}

#[test]
fn blank_lines_survive_as_empty_lines() {
    let src = "route {\n\n\texit;\n\n}\n";
    let out = format(src, &tabs());
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[1], "", "a blank line stays blank, never indented");
    assert_eq!(lines[3], "");
    assert_eq!(out.lines().count(), src.lines().count(), "line count moved");
}

#[test]
fn unbalanced_braces_never_produce_negative_indent() {
    let src = "}\n}\nroute {\nexit;\n";
    let out = format(src, &spaces(2));
    assert_eq!(out.lines().next().unwrap(), "}");
    assert!(!out.contains("  }"), "depth went negative: {out:?}");
}

#[test]
fn adversarial_input_does_not_panic() {
    for s in [
        "",
        "\n",
        "{",
        "}",
        "\"unterminated",
        "/* unterminated",
        "route {\n\u{1F600} = 1;\n}\n",
        "#",
        "\t\t\t",
    ] {
        let _ = format(s, &tabs());
        let _ = format(s, &spaces(2));
    }
}

#[test]
fn only_changed_lines_are_edited() {
    // an already-correct line must not be rewritten: a whole-document
    // replace would collapse the client's folding and cursor state
    let src = "route {\n\texit;\n}\n";
    assert!(
        format_lines(src, &tabs()).is_empty(),
        "a correctly formatted document produced edits"
    );

    let src = "route {\nexit;\n}\n";
    let edits = format_lines(src, &tabs());
    assert_eq!(edits.len(), 1, "only the misindented line needs an edit");
    assert_eq!(edits[0].line, 1);
    assert_eq!(edits[0].text, "\texit;");
}

/// The safety proof, in the form the formatter can carry on every
/// run: reformatting must not move a single thing the analyzer can
/// see.  If indentation changed what counts as a route, a load or a
/// modparam, the formatter has rewritten the config's meaning.
#[test]
fn reformatting_preserves_everything_the_analyzer_sees() {
    use opensips_lsp::analyze;
    let src = "# a config with messy indentation\n\
        loadmodule \"tm.so\"\n\
           loadmodule \"sl.so\"\n\
        modparam(\"tm\", \"fr_timeout\", 3)\n\
        \t\tmodparam(\"tm\", \"fr_timeout\", 3)\n\
        route {\n\
        xlog(\"a { brace } in a string\");\n\
        # a } comment brace\n\
        /* block { with\n\
           braces } inside */\n\
        if ($rU == \"x\") {\n\
        route(RELAY);\n\
        } else {\n\
        route(\"OTHER\");\n\
        }\n\
        }\n\
        route[RELAY] {\n\
        t_relay();\n\
        }\n\
        failure_route[RELAY] {\n\
        exit;\n\
        }\n";

    let before = (
        analyze::route_blocks(src),
        analyze::route_refs(src),
        analyze::loaded_modules(src),
        analyze::modparam_calls(src),
    );
    for opts in [tabs(), spaces(2), spaces(8)] {
        let out = format(src, &opts);
        let after = (
            analyze::route_blocks(&out),
            analyze::route_refs(&out),
            analyze::loaded_modules(&out),
            analyze::modparam_calls(&out),
        );
        assert_eq!(
            before.0.len(),
            after.0.len(),
            "route block count changed: {out}"
        );
        assert_eq!(
            before.0.iter().map(|b| &b.name).collect::<Vec<_>>(),
            after.0.iter().map(|b| &b.name).collect::<Vec<_>>(),
            "route block names changed"
        );
        assert_eq!(
            before.1.iter().map(|r| &r.name).collect::<Vec<_>>(),
            after.1.iter().map(|r| &r.name).collect::<Vec<_>>(),
            "route references changed"
        );
        assert_eq!(
            before.2.iter().map(|m| &m.name).collect::<Vec<_>>(),
            after.2.iter().map(|m| &m.name).collect::<Vec<_>>(),
            "loaded modules changed"
        );
        assert_eq!(before.3.len(), after.3.len(), "modparam call count changed");
    }
}

#[test]
fn range_formatting_touches_only_the_requested_lines() {
    use opensips_lsp::format::format_range;
    let src = "route {\nexit;\n}\nroute[A] {\nexit;\n}\n";
    let edits = format_range(src, &tabs(), 3, 5);
    assert!(
        edits.iter().all(|e| (3..=5).contains(&e.line)),
        "edits escaped the range: {edits:?}"
    );
    assert!(
        edits.iter().any(|e| e.line == 4),
        "the misindented line in range was not edited"
    );
}
