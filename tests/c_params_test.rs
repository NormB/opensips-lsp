//! The C parameter table is the list `modparam()` is checked against
//! when OpenSIPS starts, so it decides which parameters exist. These
//! prove the extraction against the shapes the 4.0.1 tree actually
//! uses — six declaration spellings, several tables in one file, and
//! the terminator and comment forms that must not be read as names.

use opensips_lsp::catalog::parse_param_export_tables;

#[test]
fn reads_the_const_params_spelling() {
    let src = r#"
static const param_export_t params[] = {
	{"rtpengine_sock",         STR_PARAM|USE_FUNC_PARAM,
				 (void*)rtpengine_set_store          },
	{"rtpengine_disable_tout", INT_PARAM, &rtpengine_disable_tout },
	{0, 0, 0}
};
"#;
    assert_eq!(
        parse_param_export_tables(src),
        vec!["rtpengine_sock", "rtpengine_disable_tout"]
    );
}

#[test]
fn reads_the_non_const_spelling() {
    let src = r#"
static param_export_t params[] = {
	{"db_url", STR_PARAM, &db_url},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["db_url"]);
}

/// `mi_xmlrpc` names its table `mi_params` and wires it into the
/// parameters slot of `struct module_exports`; `mod_params` and
/// `parameters` are the other spellings in the tree. The type is what
/// makes a table a parameter table, not the identifier.
#[test]
fn reads_every_identifier_spelling() {
    for ident in ["params", "mod_params", "mi_params", "parameters"] {
        let src = format!(
            "static const param_export_t {ident}[] = {{\n\t{{\"only\", INT_PARAM, &x}},\n\t{{0,0,0}}\n}};\n"
        );
        assert_eq!(
            parse_param_export_tables(&src),
            vec!["only"],
            "identifier {ident} must be read"
        );
    }
}

#[test]
fn unions_several_tables_in_one_file() {
    let src = r#"
static const param_export_t params[] = {
	{"first", INT_PARAM, &a},
	{0, 0, 0}
};
static const param_export_t mod_params[] = {
	{"second", INT_PARAM, &b},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["first", "second"]);
}

/// `{0, 0, 0}` and `{NULL, 0, NULL}` close a table. Reading either as
/// a parameter would put a name in the catalogue that no module
/// exports — the precise defect this change exists to remove.
#[test]
fn terminators_are_not_parameters() {
    let src = r#"
static const param_export_t params[] = {
	{"real", INT_PARAM, &a},
	{NULL, 0, NULL}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["real"]);
}

/// A commented-out entry is not an export. `modules/` carries several
/// of these, and collecting them would reintroduce phantoms by a new
/// route.
#[test]
fn commented_out_entries_are_not_parameters() {
    let src = r#"
static const param_export_t params[] = {
	{"live", INT_PARAM, &a},
	/* {"block_commented", INT_PARAM, &b}, */
	// {"line_commented", INT_PARAM, &c},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["live"]);
}

/// Only `param_export_t` tables are parameter tables. A module's
/// `cmd_export_t` names are script functions and belong to a
/// different namespace.
#[test]
fn other_export_tables_are_ignored() {
    let src = r#"
static const cmd_export_t cmds[] = {
	{"rtpengine_offer", (cmd_function)offer, {{0,0,0}}, ALL_ROUTES},
	{0, 0, {{0,0,0}}, 0}
};
static const param_export_t params[] = {
	{"the_only_param", INT_PARAM, &a},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["the_only_param"]);
}

/// A declaration that is not an array initialiser — a prototype or a
/// parameter of type `param_export_t *` — opens no table.
#[test]
fn a_bare_type_mention_opens_no_table() {
    let src = r#"
int register_params(const param_export_t *p);
static const param_export_t params[] = {
	{"actual", INT_PARAM, &a},
	{0, 0, 0}
};
"#;
    assert_eq!(parse_param_export_tables(src), vec!["actual"]);
}

#[test]
fn a_file_with_no_table_yields_nothing() {
    assert!(parse_param_export_tables("int main(void) { return 0; }").is_empty());
}

/// Only the literal that opens an entry is a name. A literal nested
/// deeper inside an entry's value is an argument.
///
/// The nesting here is a brace, deliberately: a fixture that nests the
/// literal in parentheses cannot reach the depth the guard tests, so
/// it passes whether the guard is right or wrong. This version dies
/// when the depth check is relaxed to `depth >= 2`.
#[test]
fn only_the_leading_literal_of_an_entry_is_a_name() {
    let src = r#"
static const param_export_t params[] = {
	{"paren_arg", STR_PARAM|USE_FUNC_PARAM, (void*)fixup("not_a_param")},
	{"brace_arg", STR_PARAM, &(struct opt){ "also_not_a_param", 0 }},
	{0, 0, 0}
};
"#;
    assert_eq!(
        parse_param_export_tables(src),
        vec!["paren_arg", "brace_arg"]
    );
}
