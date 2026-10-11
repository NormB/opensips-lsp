//! The outline, folding and selection ranges: what a reader navigates
//! a config by.
//!
//! A route body is where a large config's length is, so the blocks
//! inside a route nest under it in the outline and fold on their own;
//! `loadmodule`, `modparam` and top-level parameter assignments are
//! listed beside the route blocks.  Route definitions cannot nest
//! (`cfg.y` accepts them only at the top level), and a `modparam`
//! need not sit next to its `loadmodule`, so neither is nested under
//! another: an outline parent must contain its children.

use opensips_lsp::outline::{FoldKind, Sym, SymKind, folds, outline, selection_chain};

const CFG: &str = r#"loadmodule "tm.so"
modparam("tm", "fr_timeout", 5)
log_level = 3
route {
    if (is_method("INVITE")) {
        switch ($rs) {
            case 1:
                exit;
        }
    } else {
        exit;
    }
}
# a comment
# that runs on
route[relay] {
    /* one
       two */
    t_relay();
}
"#;

fn flat(syms: &[Sym], depth: usize, out: &mut Vec<String>) {
    for s in syms {
        out.push(format!(
            "{}{} {:?} {}:{}-{}:{} sel {}:{}-{}:{}",
            "  ".repeat(depth),
            s.name,
            s.kind,
            s.span.start_line,
            s.span.start_col,
            s.span.end_line,
            s.span.end_col,
            s.sel.start_line,
            s.sel.start_col,
            s.sel.end_line,
            s.sel.end_col
        ));
        flat(&s.children, depth + 1, out);
    }
}

#[test]
fn the_outline_nests_the_blocks_inside_a_route() {
    let mut got = Vec::new();
    flat(&outline(CFG), 0, &mut got);
    assert_eq!(
        got,
        vec![
            "tm Module 0:0-0:18 sel 0:12-0:17",
            "fr_timeout Property 1:0-1:31 sel 1:16-1:26",
            "log_level Variable 2:0-2:13 sel 2:0-2:9",
            "route (main) Route 3:0-12:1 sel 3:0-3:5",
            "  if (is_method(\"INVITE\")) Block 4:4-9:5 sel 4:4-4:6",
            "    switch ($rs) Block 5:8-8:9 sel 5:8-5:14",
            "  else Block 9:6-11:5 sel 9:6-9:10",
            "route[relay] Route 15:0-19:1 sel 15:6-15:11",
        ]
    );
}

#[test]
fn a_modparam_keeps_its_module_as_detail() {
    let syms = outline(CFG);
    let p = syms.iter().find(|s| s.name == "fr_timeout").unwrap();
    assert_eq!(p.detail.as_deref(), Some("tm"));
}

#[test]
fn only_top_level_assignments_are_parameters() {
    // `$var(x) = 1` inside a route is a script assignment, and a
    // line inside a route body that opens with `word =` is not a
    // global parameter whatever else it is
    let t = "debug_mode=yes\nroute {\n    $var(x) = 1;\n    if ($var(x) == 1 &&\n    stray = 2) { exit; }\n}\n";
    let names: Vec<String> = outline(t)
        .into_iter()
        .filter(|s| s.kind == SymKind::Variable)
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["debug_mode"]);
}

#[test]
fn folds_cover_nested_blocks_and_comments() {
    let got: Vec<(u32, u32, FoldKind)> = folds(CFG)
        .into_iter()
        .map(|f| (f.start_line, f.end_line, f.kind))
        .collect();
    assert_eq!(
        got,
        vec![
            (3, 12, FoldKind::Region),
            // `} else {` closes this block on a line that opens the
            // next, so the fold stops above it and `else` stays visible
            (4, 8, FoldKind::Region),
            (5, 8, FoldKind::Region),
            (9, 11, FoldKind::Region),
            (13, 14, FoldKind::Comment),
            (15, 19, FoldKind::Region),
            (16, 17, FoldKind::Comment),
        ]
    );
}

#[test]
fn a_brace_in_a_string_or_comment_opens_no_block() {
    let t = "route {\n    xlog(\"{\");\n    # {\n    exit;\n}\n";
    let got: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(got, vec![(0, 4)]);
}

#[test]
fn selection_grows_from_the_word_to_the_route() {
    // the cursor on `rs` in `switch ($rs)`
    let line5 = CFG.lines().nth(5).unwrap();
    let off = CFG.lines().take(5).map(|l| l.len() + 1).sum::<usize>() + line5.find("rs").unwrap();
    let got: Vec<String> = selection_chain(CFG, off)
        .into_iter()
        .map(|s| {
            format!(
                "{}:{}-{}:{}",
                s.start_line, s.start_col, s.end_line, s.end_col
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            "5:17-5:19", // rs
            "5:8-8:9",   // the switch statement
            "4:4-9:5",   // the if statement holding it
            "3:0-12:1",  // the route block
            "0:0-20:0",  // the document
        ]
    );
}

#[test]
fn selection_outside_every_block_is_word_then_document() {
    let got: Vec<String> = selection_chain(CFG, 3)
        .into_iter()
        .map(|s| {
            format!(
                "{}:{}-{}:{}",
                s.start_line, s.start_col, s.end_line, s.end_col
            )
        })
        .collect();
    assert_eq!(got, vec!["0:0-0:10", "0:0-20:0"]);
}

#[test]
fn a_closing_brace_in_a_string_closes_nothing() {
    let t = "route {\n    if ($rU) {\n        xlog(\"}\");\n        exit;\n    }\n}\n";
    let got: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(got, vec![(0, 5), (1, 4)]);
}

#[test]
fn a_comment_after_a_closing_brace_is_not_more_code() {
    let t = "route {\n    if ($rU) {\n        exit;\n    } # done\n}\n";
    let got: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(got, vec![(0, 4), (1, 3)]);
}

#[test]
fn a_comment_above_a_block_is_not_part_of_its_name() {
    let t = "route {\n    exit;\n    # only INVITEs\n    if (is_method(\"INVITE\")) {\n        exit;\n    }\n}\n";
    let route = outline(t).pop().unwrap();
    let b = &route.children[0];
    assert_eq!(b.name, "if (is_method(\"INVITE\"))");
    assert_eq!((b.span.start_line, b.span.start_col), (3, 4));
}

#[test]
fn one_fold_per_start_line_and_it_is_the_widest() {
    let t = "route { if ($rU) {\n    exit;\n}\n}\n";
    let got: Vec<(u32, u32)> = folds(t)
        .into_iter()
        .map(|f| (f.start_line, f.end_line))
        .collect();
    assert_eq!(got, vec![(0, 3)]);
}
