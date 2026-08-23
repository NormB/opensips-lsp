use opensips_lsp::diag::{Severity, parse_check_output};

// Verbatim opensips -C output captured from a real failure
// (test_outage_matrix_e2e.sh, 2026-08-19).
const REAL: &str = r#"Aug 19 00:57:10 [2288585] CRITICAL:core:yyerror: parse error in /tmp/outage_matrix.Vx9OYq/test.cfg:26:19-20: Parameter <enable_search_index> not found in module <cachedb_nats> - can't set
Aug 19 00:57:10 [2288585] CRITICAL:modparam("cachedb_nats", "kv_replicas", 1)
Aug 19 00:57:10 [2288585] CRITICAL:modparam("cachedb_nats", "enable_search_index", 1)
Aug 19 00:57:10 [2288585] CRITICAL:^~
Aug 19 00:57:10 [2288585] ERROR:core:parse_opensips_cfg: bad config file (1 errors)
Aug 19 00:57:10 [2288585] ERROR:core:main: failed to parse config file /tmp/outage_matrix.Vx9OYq/test.cfg
"#;

const REAL_LOAD: &str = r#"Aug 19 02:08:16 [2345829] ERROR:core:load_module: failed to load module 'mi_fifo.so' - not found
Aug 19 02:08:16 [2345829] CRITICAL:core:yyerror: parse error in /tmp/crud_e2e.BwSrLB/test.cfg:18:13-14: failed to load module mi_fifo.so
"#;

#[test]
fn parses_positioned_yyerror() {
    let ds = parse_check_output(REAL, 255);
    assert_eq!(ds.len(), 1);
    let d = &ds[0];
    assert_eq!(d.file, "/tmp/outage_matrix.Vx9OYq/test.cfg");
    assert_eq!(d.line, 25); // 1-based 26 -> 0-based 25
    assert_eq!(d.col_start, 18); // 1-based 19 -> 18
    assert_eq!(d.col_end, 19); // exclusive: the token occupies col 19 only
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("enable_search_index"));
    assert!(!d.message.contains("yyerror")); // internal tag stripped
}

#[test]
fn load_module_failure_uses_the_positioned_line() {
    let ds = parse_check_output(REAL_LOAD, 255);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].line, 17);
    assert!(ds[0].message.contains("failed to load module mi_fifo.so"));
}

#[test]
fn clean_output_zero_rc_is_empty() {
    let out = "Listening on\n  udp: 127.0.0.1 [127.0.0.1]:5060\nAliases:\n";
    assert!(parse_check_output(out, 0).is_empty());
}

#[test]
fn nonzero_rc_with_no_positioned_error_yields_fallback() {
    let out = "Aug 19 [1] ERROR:core:main: something exploded\n";
    let ds = parse_check_output(out, 255);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].line, 0);
    assert!(ds[0].message.contains("something exploded"));
}

#[test]
fn adversarial_output_does_not_panic() {
    for s in [
        "",
        "\0\0",
        "CRITICAL:core:yyerror: parse error in x.cfg:99999999999999999999:5-6: overflow line",
        "CRITICAL:core:yyerror: parse error in :0:0-0:",
        "parse error in a.cfg:3:9-8: reversed cols",
        "no colons at all",
    ] {
        let _ = parse_check_output(s, 1);
    }
}

#[test]
fn column_range_never_reversed() {
    let ds = parse_check_output(
        "CRITICAL:core:yyerror: parse error in a.cfg:3:9-8: reversed\n",
        1,
    );
    if let Some(d) = ds.first() {
        assert!(d.col_end >= d.col_start);
    }
}

// Verbatim opensips 3.6.8 -C output (captured 2026-08-19).  The
// positioned-error format is identical to 4.x — see REAL_401 below,
// captured from 4.0.1 — and the parser must keep handling both.
const REAL_368: &str = r#"Aug 19 14:20:53 [2737664] CRITICAL:core:yyerror: parse error in /tmp/x/proof36.cfg:2:13-14: failed to load module tm.so
Aug 19 14:20:53 [2737664] CRITICAL:core:yyerror: parse error in /tmp/x/proof36.cfg:3:19-20: Parameter <no_such_param_xyz> not found in module <tm> - can't set
Aug 19 14:20:53 [2737664] ERROR:core:parse_opensips_cfg: bad config file (2 errors)
"#;

#[test]
fn parses_368_output_identically() {
    let ds = parse_check_output(REAL_368, 255);
    assert_eq!(ds.len(), 2);
    assert_eq!(ds[0].line, 1);
    assert!(ds[1].message.contains("no_such_param_xyz"));
}

// Verbatim opensips 4.0.1 -C output (captured 2026-08-23), with the
// module actually loaded so the modparam failure is the real one.
// 4.x wraps the positioned error in context 3.6 never printed: a
// traceback header, an include-depth line, a caret marker, and an
// echo of the offending config LINES.  All of it is noise the parser
// has to walk past — the echoed config text especially, since it is
// arbitrary user input sitting behind a `CRITICAL:` prefix.
const REAL_401: &str = r#"Aug 23 11:22:12 [2299450] ERROR:core:set_mod_param_regex: parameter <no_such_param_xyz> not found in module <tm>
Aug 23 11:22:12 [2299450] CRITICAL:Traceback (last included file at the bottom):
Aug 23 11:22:12 [2299450] CRITICAL: 0. /tmp/ostest/proof40.cfg
Aug 23 11:22:12 [2299450] CRITICAL:core:yyerror: parse error in /tmp/ostest/proof40.cfg:4:19-20: Parameter <no_such_param_xyz> not found in module <tm> - can't set
Aug 23 11:22:12 [2299450] CRITICAL:mpath="/tmp/ostest/mods/"
Aug 23 11:22:12 [2299450] CRITICAL:loadmodule "tm.so"
Aug 23 11:22:12 [2299450] CRITICAL:modparam("tm", "no_such_param_xyz", 1)
Aug 23 11:22:12 [2299450] CRITICAL:^~
Aug 23 11:22:12 [2299450] CRITICAL:route { exit; }
Aug 23 11:22:12 [2299450] ERROR:core:parse_opensips_cfg: bad config file (1 errors)
Aug 23 11:22:12 [2299450] ERROR:core:main: failed to parse config file /tmp/ostest/proof40.cfg
Aug 23 11:22:12 [2299450] NOTICE:core:main: Exiting....
"#;

#[test]
fn parses_401_output_and_ignores_the_4x_context_block() {
    let ds = parse_check_output(REAL_401, 255);
    assert_eq!(
        ds.len(),
        1,
        "the traceback, caret and config echo are not diagnostics: {ds:?}"
    );
    let d = &ds[0];
    assert_eq!(d.file, "/tmp/ostest/proof40.cfg");
    assert_eq!(d.line, 3); // 1-based 4 -> 0-based 3
    assert_eq!(d.col_start, 18); // 1-based 19 -> 18
    assert_eq!(d.col_end, 19); // end is EXCLUSIVE, as in 3.6
    assert!(d.message.contains("no_such_param_xyz"));
}

#[test]
fn absurdly_long_messages_are_truncated() {
    let long = "x".repeat(50_000);
    let out = format!("CRITICAL:core:yyerror: parse error in a.cfg:1:1-2: {long}\n");
    let ds = parse_check_output(&out, 255);
    assert_eq!(ds.len(), 1);
    assert!(
        ds[0].message.len() <= 600,
        "message must be bounded, got {} bytes",
        ds[0].message.len()
    );
    assert!(ds[0].message.ends_with('…'), "truncation must be visible");
}

#[test]
fn end_column_is_exclusive_in_opensips_output() {
    // real binary evidence: the 6-char token `listen` at 1-based
    // columns 1-6 is reported as `1-7` — the end is EXCLUSIVE
    let out = "CRITICAL:core:yyerror: parse error in t.cfg:2:1-7: syntax error\n";
    let ds = parse_check_output(out, 255);
    assert_eq!(ds.len(), 1);
    assert_eq!(ds[0].col_start, 0);
    assert_eq!(
        ds[0].col_end, 6,
        "0-based exclusive end must cover exactly the 6-char token"
    );
    // degenerate zero-width report still yields a visible 1-char range
    let out = "CRITICAL:core:yyerror: parse error in t.cfg:2:5-5: x\n";
    let ds = parse_check_output(out, 255);
    assert_eq!((ds[0].col_start, ds[0].col_end), (4, 5));
}

#[test]
fn nonzero_rc_with_unparseable_output_still_yields_a_diag() {
    // a crashed/garbled checker must still surface SOMETHING
    for garbage in ["", "segfault\n", "\0\0\0\n", "not a real line"] {
        let ds = parse_check_output(garbage, 255);
        assert_eq!(ds.len(), 1, "input {garbage:?}");
        assert!(ds[0].message.contains("rc=255") || !ds[0].message.is_empty());
    }
    // rc=0 with garbage stays clean
    assert!(parse_check_output("noise\n", 0).is_empty());
}
