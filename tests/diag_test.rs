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
    assert_eq!(d.col_end, 20); // exclusive end covering col 20
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

// Verbatim opensips 3.6.8 -C output (captured 2026-08-19): the format
// is identical to 4.x, and the parser must keep handling it.
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
