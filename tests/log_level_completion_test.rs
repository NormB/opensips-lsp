//! Typing `xlog(` offers the log levels.
//!
//! GIVEN `xlog`'s first argument is one of a fixed set the C parser
//! recognises by its third character,
//! WHEN a reader types the open parenthesis,
//! THEN the editor offers that set, spelled the way the grammar
//! accepts it.
//!
//! Nothing was offered at all, and the set is not written down in the
//! documentation in any form a harvester can read: it is a `switch`
//! in `route.c`, and it differs between releases and between the two
//! servers. So it is read from there, the same way module parameters
//! are read from `param_export_t` rather than from prose.
//!
//! The grammar spells the argument `STRING`, so the quotes are part
//! of the answer: `xlog(L_INFO, ...)` is a syntax error. Offering a
//! bare `L_INFO` would complete straight into one, so the quotes are
//! inserted with the level — unless the reader has already typed the
//! opening quote, in which case adding another makes `""L_INFO"`.

mod common;

use opensips_lsp::catalog::parse_log_levels_c;
use opensips_lsp::logic::completions_with_core;

const SWITCH: &str = r#"
	else
	{
		xlp->type = 0;
		switch(s.s[2])
		{
			case 'A': xlp->v.level = L_ALERT; break;
			case 'C': xlp->v.level = L_CRIT; break;
			case 'E': xlp->v.level = L_ERR; break;
			default:
				LM_ERR("unknown log level\n");
				return E_UNSPEC;
		}
	}
"#;

#[test]
fn the_levels_come_from_the_c_switch_that_parses_them() {
    let got = parse_log_levels_c(SWITCH);
    assert_eq!(got, vec!["L_ALERT", "L_CRIT", "L_ERR"]);
}

#[test]
fn a_switch_that_is_not_the_level_switch_is_not_read() {
    let other = r#"
		switch(s.s[0])
		{
			case 'A': x = L_ALERT; break;
			case 'B': x = L_BOGUS; break;
		}
	"#;
    assert!(
        parse_log_levels_c(other).is_empty(),
        "only the switch whose default arm says `unknown log level` is the \
         level switch — any other `case 'X': ... L_Y` in the tree would \
         otherwise be swept in"
    );
}

#[test]
fn a_case_letter_that_contradicts_its_level_is_skipped() {
    let wrong = r#"
		switch(s.s[2])
		{
			case 'A': xlp->v.level = L_ALERT; break;
			case 'Z': xlp->v.level = L_ERR; break;
			default:
				LM_ERR("unknown log level\n");
		}
	"#;
    assert_eq!(
        parse_log_levels_c(wrong),
        vec!["L_ALERT"],
        "the parser dispatches on the third character, so a case letter that \
         is not that character means the shape has changed and the pairing \
         cannot be trusted"
    );
}

#[test]
fn the_real_tree_yields_the_levels_opensips_accepts() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let core = opensips_lsp::catalog::harvest_core(std::path::Path::new(&tree));
    assert_eq!(
        core.log_levels,
        vec![
            "L_ALERT", "L_CRIT", "L_ERR", "L_WARN", "L_NOTICE", "L_INFO", "L_DBG"
        ],
        "read from the tree's own switch, in the order it lists them"
    );
}

#[test]
fn the_built_in_catalogue_carries_the_levels() {
    let core = &opensips_lsp::catalog::builtin_core().core;
    assert!(
        core.log_levels.contains(&"L_INFO".to_string()),
        "levels must ship, or the offer needs a source checkout: {:?}",
        core.log_levels
    );
}

fn labels(prefix: &str) -> Vec<String> {
    let core = &opensips_lsp::catalog::builtin_core().core;
    completions_with_core(&[], core, "route {\n}\n", prefix)
        .into_iter()
        .map(|c| c.label)
        .collect()
}

#[test]
fn an_open_parenthesis_offers_the_levels_quoted() {
    let got = labels("    xlog(");
    assert!(
        got.contains(&"\"L_INFO\"".to_string()),
        "the grammar wants a STRING there, so the quotes come with it: {got:?}"
    );
    assert!(
        !got.contains(&"L_INFO".to_string()),
        "and the bare form must not be offered beside it — picking it writes a \
         syntax error: {got:?}"
    );
}

#[test]
fn inside_an_opened_quote_the_levels_are_offered_bare() {
    let got = labels("    xlog(\"");
    assert!(
        got.contains(&"L_INFO".to_string()),
        "the reader has typed the quote already: {got:?}"
    );
    assert!(
        !got.contains(&"\"L_INFO\"".to_string()),
        "adding another pair gives `\"\"L_INFO\"`: {got:?}"
    );
}

#[test]
fn the_second_argument_is_a_format_string_and_offers_no_levels() {
    let got = labels("    xlog(\"L_INFO\", \"");
    assert!(
        !got.iter().any(|l| l.contains("L_INFO")),
        "the level belongs to the first argument only: {got:?}"
    );
}

#[test]
fn a_function_without_a_level_argument_offers_none() {
    let got = labels("    xdbg(\"");
    assert!(
        !got.iter().any(|l| l.contains("L_INFO")),
        "xdbg takes a format and no level: {got:?}"
    );
}

#[test]
fn outside_any_call_no_levels_are_offered() {
    let got = labels("    ");
    assert!(
        !got.iter().any(|l| l.contains("L_INFO")),
        "a level is only ever an argument: {got:?}"
    );
}

/// Owed for the regression above: putting the level offer ahead of
/// the `$` branch replaced pseudo-variable completion inside
/// `xlog("$` with a list of levels — and the C parser takes a
/// pseudo-variable as the level (`s.s[0]==PV_MARKER`), so both belong
/// there. The typed `$` is what says which the reader wants.
#[test]
fn a_dollar_inside_the_level_argument_still_offers_pseudo_variables() {
    let got = labels("    xlog(\"$");
    assert!(
        got.iter().any(|l| l.starts_with('$')),
        "the level argument accepts a pseudo-variable: {got:?}"
    );
    assert!(
        !got.iter().any(|l| l.contains("L_INFO")),
        "and having typed `$` the reader is not asking for a level: {got:?}"
    );
}

#[test]
fn a_dollar_in_the_format_argument_still_offers_pseudo_variables() {
    let got = labels("    xlog(\"L_INFO\", \"got $");
    assert!(
        got.iter().any(|l| l.starts_with('$')),
        "a format string is where pseudo-variables are interpolated: {got:?}"
    );
}

/// The second owed test. The offer and signature help share one walk
/// of the line, so they cannot disagree about which argument the
/// cursor is in — and that walk must not count a comma inside a
/// string. `xlog("hello, world` is still the FIRST argument, and a
/// walk that miscounts it silently stops offering there.
#[test]
fn a_comma_inside_a_string_does_not_advance_the_argument() {
    let got = labels("    xlog(\"hello, world");
    assert!(
        got.contains(&"L_INFO".to_string()),
        "still the first argument, so the levels still belong: {got:?}"
    );
    let site = opensips_lsp::logic::call_site("    xlog(\"hello, world").expect("in a call");
    assert_eq!(site.arg, 0, "the comma is text, not a separator");
    assert!(site.in_string, "and the cursor is inside the string");
    // POSITIVE CONTROL: a comma OUTSIDE the string does advance it
    let site = opensips_lsp::logic::call_site("    xlog(\"lvl\", \"fmt").expect("in a call");
    assert_eq!(site.arg, 1, "a real separator advances the index");
}
