use opensips_lsp::analyze::{loaded_modules, modparam_context, route_defs, route_refs, word_at};

const CFG: &str = r#"# reachability-only config
loadmodule "proto_udp.so"
loadmodule "tm.so"
#loadmodule "commented_out.so"
/* loadmodule "block_commented.so" */
loadmodule "cachedb_nats.so"

modparam("cachedb_nats", "kv_bucket", "OS_USRLOC")
modparam("cachedb_nats", "nats_url", "nats://10.0.0.31:4222")

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
