use opensips_lsp::analyze::{
    includes, loaded_modules, modparam_calls, modparam_context, pvars, route_blocks, route_defs,
    route_refs, word_at,
};

const CFG: &str = r#"# reachability-only config
loadmodule "proto_udp.so"
loadmodule "tm.so"
#loadmodule "commented_out.so"
/* loadmodule "block_commented.so" */
loadmodule "cachedb_nats.so"

modparam("cachedb_nats", "kv_bucket", "OS_USRLOC")
modparam("cachedb_nats", "nats_url", "nats://192.0.2.10:4222")

route
{
    if (is_method("OPTIONS")) {
        send_reply(200, "OK");   # has a # inside a line with strings
        exit;
    }
    route(check_source);
    xlog("hash # inside string is not a comment: loadmodule \"fake.so\"");
}

route[check_source]
{
    if (!check_source_address(0)) {
        send_reply(403, "Forbidden");
    }
}

failure_route[nat_fail] {
    t_reply(500, "err");
}
"#;

#[test]
fn finds_loaded_modules_skipping_comments() {
    let mods: Vec<String> = loaded_modules(CFG).into_iter().map(|m| m.name).collect();
    assert_eq!(mods, vec!["proto_udp", "tm", "cachedb_nats"]);
}

#[test]
fn loadmodule_inside_string_is_not_collected() {
    let mods = loaded_modules(CFG);
    assert!(!mods.iter().any(|m| m.name == "fake"));
}

#[test]
fn loadmodule_positions_are_line_accurate() {
    let mods = loaded_modules(CFG);
    assert_eq!(mods[0].line, 1); // 0-based: second line of file
    assert_eq!(mods[2].line, 5);
}

#[test]
fn finds_route_definitions() {
    let defs = route_defs(CFG);
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"")); // main route
    assert!(names.contains(&"check_source"));
    assert!(names.contains(&"nat_fail"));
    let cs = defs.iter().find(|d| d.name == "check_source").unwrap();
    assert_eq!(cs.line, 20);
}

#[test]
fn finds_route_references() {
    let refs = route_refs(CFG);
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].name, "check_source");
    assert_eq!(refs[0].line, 16);
}

#[test]
fn modparam_context_detects_module_of_param_position() {
    // inside the second string of a modparam → completion should offer
    // params of that module
    let line = r#"modparam("cachedb_nats", "kv_"#;
    assert_eq!(modparam_context(line), Some("cachedb_nats".to_string()));
    assert_eq!(
        modparam_context(r#"modparam("tm", "#),
        Some("tm".to_string())
    );
    assert_eq!(modparam_context("xlog(\"nope\""), None);
    // first argument still open → no module context yet
    assert_eq!(modparam_context(r#"modparam("cache"#), None);
}

#[test]
fn word_at_extracts_identifier() {
    let text = "    send_reply(200, \"OK\");";
    assert_eq!(word_at(text, 6), Some("send_reply".to_string()));
    assert_eq!(word_at(text, 0), None);
    assert_eq!(word_at("", 5), None);
    // out-of-range column must not panic
    assert_eq!(word_at("ab", 99), None);
}

#[test]
fn adversarial_inputs_do_not_panic() {
    for s in [
        "",
        "\0\0\0",
        "loadmodule \"\0.so\"",
        "route[",
        "#",
        "\\",
        "modparam(\"",
        "route[\\\"x]",
    ] {
        let _ = loaded_modules(s);
        let _ = route_defs(s);
        let _ = route_refs(s);
        let _ = modparam_context(s);
        let _ = word_at(s, 3);
    }
}

#[test]
fn route_blocks_report_full_extents() {
    let text = "loadmodule \"tm.so\"\nroute {\n    if (1) {\n        exit;\n    }\n}\nfailure_route[fr] {\n    xlog(\"x\");\n}\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].kind, "route");
    assert_eq!(blocks[0].name, "");
    assert_eq!((blocks[0].line, blocks[0].col), (1, 0));
    // nested braces are matched: the block ends at line 5's `}`
    assert_eq!((blocks[0].end_line, blocks[0].end_col), (5, 1));
    assert_eq!(blocks[1].kind, "failure_route");
    assert_eq!(blocks[1].name, "fr");
    assert_eq!((blocks[1].line, blocks[1].end_line), (6, 8));
}

#[test]
fn route_blocks_ignore_braces_in_strings_and_comments() {
    let text = "route {\n    xlog(\"}\");\n    # }\n    /* } */\n    exit;\n}\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!((blocks[0].end_line, blocks[0].end_col), (5, 1));
}

#[test]
fn unterminated_route_block_extends_to_eof() {
    let text = "route {\n    exit;\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].end_line, 2);
}

#[test]
fn route_blocks_adversarial_do_not_panic() {
    for s in [
        "",
        "route {",
        "route }{",
        "route {{{{",
        "route[\u{0}] {}",
        "route[a] { \"unterminated",
        "}}}}",
    ] {
        let _ = route_blocks(s);
    }
}

#[test]
fn includes_are_extracted_from_code_position_only() {
    let text = "include_file \"a.cfg\"\nimport_file \"sub/b.cfg\"\n# include_file \"no.cfg\"\n/* include_file \"no2.cfg\" */\nxlog(\"include_file \\\"no3.cfg\\\"\");\n";
    let inc = includes(text);
    let names: Vec<&str> = inc.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["a.cfg", "sub/b.cfg"]);
    assert_eq!(inc[0].line, 0);
    assert_eq!(inc[1].line, 1);
}

#[test]
fn includes_adversarial_do_not_panic() {
    for s in [
        "",
        "include_file",
        "include_file \"",
        "include_file \"\0\"",
        "import_file \"\\\"\"",
        "reinclude_file \"x\"",
    ] {
        let _ = includes(s);
    }
    // an identifier tail must not match (reinclude_file)
    assert!(includes("reinclude_file \"x.cfg\"").is_empty());
}

#[test]
fn timer_route_with_interval_is_recognized() {
    // OpenSIPS grammar REQUIRES the interval: timer_route[name, N] { }
    let text = "timer_route[gw_update, 30] {\n    exit;\n}\n";
    let defs = route_defs(text);
    assert_eq!(defs.len(), 1, "{defs:?}");
    assert_eq!(defs[0].name, "gw_update");
    let blocks = route_blocks(text);
    assert_eq!(blocks[0].kind, "timer_route");
    assert_eq!(blocks[0].end_line, 2);
    // space-free variant
    assert_eq!(route_defs("timer_route[t,30]{exit;}\n").len(), 1);
    // interval must not leak into the name
    assert!(route_defs(text).iter().all(|d| !d.name.contains("30")));
}

#[test]
fn single_quoted_strings_are_strings() {
    // OpenSIPS accepts single-quoted strings; '#', '{', '}' inside
    // them must not derail comment detection or brace matching
    let text = "route {\n    xlog('hello # } {');\n    exit;\n}\nroute[sq] { exit; }\n";
    let blocks = route_blocks(text);
    assert_eq!(blocks.len(), 2, "{blocks:?}");
    assert_eq!(blocks[0].end_line, 3);
    // single-quoted route names in defs and refs
    let text = "route['sq'] { exit; }\nroute {\n    route('sq');\n}\n";
    assert!(route_defs(text).iter().any(|d| d.name == "sq"));
    assert!(route_refs(text).iter().any(|r| r.name == "sq"));
    // and single-quoted includes
    let inc = includes("include_file 'a.cfg'\n");
    assert_eq!(inc.len(), 1);
    assert_eq!(inc[0].name, "a.cfg");
    // adversarial: unterminated single quote
    for s in ["'", "x'", "route['", "xlog('a\\'b');"] {
        let _ = route_blocks(s);
        let _ = includes(s);
    }
}

#[test]
fn kamailio_only_route_kinds_are_not_opensips_defs() {
    // request_route / reply_route / onsend_route are Kamailio blocks;
    // in OpenSIPS they must not register as route definitions (the
    // word-boundary guard must also keep `request_route {` from
    // matching its `route` suffix)
    for cfg in [
        "request_route {\n    exit;\n}\n",
        "reply_route {\n    exit;\n}\n",
        "onsend_route {\n    exit;\n}\n",
    ] {
        assert!(
            route_defs(cfg).is_empty(),
            "kamailio-only kind must not define a route: {cfg:?}"
        );
    }
}

#[test]
fn modparam_calls_are_extracted_with_param_positions() {
    let text = "modparam(\"tm\", \"fr_timeout\", 5)\n# modparam(\"no\", \"x\", 1)\nmodparam('sq', 'p2', \"v\")\n";
    let mp = modparam_calls(text);
    assert_eq!(mp.len(), 2, "{mp:?}");
    assert_eq!(
        (mp[0].module.as_str(), mp[0].param.as_str()),
        ("tm", "fr_timeout")
    );
    assert_eq!(mp[0].line, 0);
    // col points at the PARAM name (inside its quotes)
    assert_eq!(mp[0].col, 16);
    assert_eq!((mp[1].module.as_str(), mp[1].param.as_str()), ("sq", "p2"));
    // adversarial
    for s in [
        "",
        "modparam(",
        "modparam(\"a\"",
        "modparam(\"a\",\"\0\",1)",
    ] {
        let _ = modparam_calls(s);
    }
}

#[test]
fn pvars_are_classifier_scoped() {
    // strings (both quote styles) keep pvars; comments drop them
    let text =
        "xlog(\"call #1: $ru\");\nxlog('#: $rd');\n# $avp(x)\n/* $var(y) */ $rU;\n/*\n $si\n*/\n";
    let got = pvars(text);
    assert_eq!(
        got,
        vec![(0, 15, 3), (1, 9, 3), (3, 14, 3)],
        "in-string pvars stay, commented pvars go: {got:?}"
    );
    // multibyte before the pvar: col is a BYTE column
    let got = pvars("xlog(\"héllo $ru\");\n");
    assert_eq!(got, vec![(0, 13, 3)]);
    // adversarial: NUL, backslash escapes, unterminated — no panic
    for s in [
        "",
        "$",
        "\0$ru",
        "\"\\\\$ru",
        "/* $ru",
        "'$ru",
        "$ru\u{7f}$rd",
    ] {
        let _ = pvars(s);
    }
}

/// OpenSIPS block comments nest: cfg.lex counts `comment_nest` up on
/// every `/*` inside a comment and leaves the comment only when it
/// returns to zero.  `opensips -C` 4.0.2 accepts the first config
/// below, so the `route(ghost)` inside it is not a call.
#[test]
fn block_comments_nest() {
    let t = "/* outer /* inner */ route(ghost); */\n\
             route[real] {\n    route(after);\n}\n\
             /* a /* b */ route[gone] { exit; } */\n";
    let refs: Vec<String> = route_refs(t).into_iter().map(|r| r.name).collect();
    assert_eq!(refs, vec!["after"]);
    let defs: Vec<String> = route_defs(t).into_iter().map(|d| d.name).collect();
    assert_eq!(defs, vec!["real"]);
}

#[test]
fn the_cursor_is_in_a_comment_only_before_the_comment_ends() {
    use opensips_lsp::analyze::in_comment;
    let at = |t: &str, needle: &str| t.find(needle).unwrap() + needle.len();
    let t = "route {\n    # xlo\n    xlo\n    /* a /* b */ xl */ xlo\n}\n";
    assert!(in_comment(t, at(t, "# xlo")), "inside a # comment");
    assert!(!in_comment(t, at(t, "    xlo\n") - 1), "plain code");
    assert!(in_comment(t, at(t, "b */ xl")), "still inside the outer /*");
    assert!(!in_comment(t, at(t, "xl */")), "after the outer */ closes");
    assert!(!in_comment(t, at(t, "*/ xlo")), "code after the comment");
    // a `#` or `/*` inside a string opens no comment
    let s = "route {\n    xlog(\"# /* x\"); xl\n}\n";
    assert!(!in_comment(s, at(s, "# /* x")));
    assert!(!in_comment(s, at(s, "; xl")));
    // the end of a `#` line is still in it; the next line is not
    let e = "# c\nxl";
    assert!(in_comment(e, 3));
    assert!(!in_comment(e, e.len()));
}

#[test]
fn a_backslash_in_single_quotes_escapes_nothing() {
    // cfg.lex STRING2 ends at the next `'` and has no escapes, so
    // `'\'` is a complete string and the route call after it is code
    let t = "route {\n    xlog('\\'); route(after);\n}\n";
    let refs: Vec<String> = route_refs(t).into_iter().map(|r| r.name).collect();
    assert_eq!(refs, vec!["after"]);
}

#[test]
fn loadmodule_takes_a_single_quoted_string_too() {
    // cfg.y `module_stm: LOADMODULE STRING`, and cfg.lex makes a STRING
    // of either quote; `opensips -C` resolves t_relay after
    // `loadmodule 'tm.so'` and fails to parse without it
    let t = "loadmodule 'tm.so'\nloadmodule \"sl.so\"\n# loadmodule 'gone.so'\n";
    let names: Vec<String> = loaded_modules(t).into_iter().map(|m| m.name).collect();
    assert_eq!(names, vec!["tm", "sl"]);
}

/// cfg.lex reads `$` in state SCRIPTVARS and, while a parenthesis is
/// open (`np` > 0), takes every byte as part of the variable
/// (`<SCRIPTVARS>.  { yymore(); }`): a `#` or a quote in
/// `$(var(a){re.subst,/"//g})` opens no comment and no string.
/// `tests/grammar/pvar_transformations.cfg` holds the same constructs
/// to `opensips -C` 4.0.2.
#[test]
fn a_script_variable_body_opens_no_comment_or_string() {
    use opensips_lsp::analyze::in_comment;
    let t = "route {\n\
             \t$ru = $(ru{re.subst,/;transport=[A-Za-z]*#//g});\n\
             \t$var(a) = $(var(a){re.subst,/\"//g});\n\
             \t$var(c) = $hdr(X#Y);\n\
             \troute(after);\n\
             }\n\
             route[after] {\n\texit; # real\n}\n";
    let spans: Vec<(String, u32, u32)> = route_blocks(t)
        .into_iter()
        .map(|b| (b.name, b.line, b.end_line))
        .collect();
    assert_eq!(spans, vec![("".into(), 0, 5), ("after".into(), 6, 8)]);
    let refs: Vec<String> = route_refs(t).into_iter().map(|r| r.name).collect();
    assert_eq!(refs, vec!["after"]);
    let at = |needle: &str| t.find(needle).unwrap() + needle.len();
    assert!(!in_comment(t, at("]*#")), "`#` inside $(...)");
    assert!(!in_comment(t, at("X#")), "`#` inside $hdr(...)");
    assert!(in_comment(t, at("# re")), "a `#` after the variable closes");
}

/// A `$(` with no `)` on its line ends there: one being typed must not
/// turn the rest of the document into variable text.
#[test]
fn an_unclosed_script_variable_ends_at_its_line() {
    let t = "route {\n\t$var(a) = $(ru{s.len\n}\nroute[n] {\n\texit;\n}\n";
    let spans: Vec<(String, u32, u32)> = route_blocks(t)
        .into_iter()
        .map(|b| (b.name, b.line, b.end_line))
        .collect();
    assert_eq!(spans, vec![("".into(), 0, 2), ("n".into(), 3, 5)]);
}

/// A variable nested in another's parentheses is still a variable to
/// highlight, though cfg.lex reads it as text of the outer one.
#[test]
fn a_script_variable_nested_in_another_is_still_reported() {
    let t = "$var(x) = $hdr($var(n));";
    let got: Vec<&str> = pvars(t)
        .into_iter()
        .map(|(_, c, n)| &t[c as usize..(c + n) as usize])
        .collect();
    assert_eq!(got, vec!["$var(x)", "$hdr", "$var(n)"]);
}
