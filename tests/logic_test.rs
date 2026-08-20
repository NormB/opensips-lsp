use opensips_lsp::catalog::{Item, ModuleDoc};
use opensips_lsp::logic::{
    CompKind, SemKind, analyzer_diagnostics, attribute_foreign_diag, catalog_diagnostics,
    completions, completions_with_core, definition_of, encode_semantic_tokens, hover_markdown,
    include_closure, loaded_modules_multi, pvar_tail, quick_fixes, route_defs_multi,
    route_occurrences, route_symbol_at, semantic_spans, signature_at, split_params,
    valid_route_name,
};

fn catalog() -> Vec<ModuleDoc> {
    vec![
        ModuleDoc {
            name: "tm".into(),
            params: vec![Item {
                name: "fr_timeout".into(),
                detail: "integer".into(),
                doc: "Final response timeout.".into(),
            }],
            functions: vec![Item {
                name: "t_relay".into(),
                detail: "t_relay([flags],[outbound_proxy])".into(),
                doc: "Relay statefully.".into(),
            }],
        },
        ModuleDoc {
            name: "cachedb_nats".into(),
            params: vec![Item {
                name: "kv_bucket".into(),
                detail: "string".into(),
                doc: "Bucket name.".into(),
            }],
            functions: vec![Item {
                name: "nats_kv_get".into(),
                detail: "nats_kv_get(bucket, key, value_pvar)".into(),
                doc: "Gets a value.".into(),
            }],
        },
    ]
}

const DOC: &str =
    "loadmodule \"tm.so\"\n\nroute[check] {\n    t_relay();\n}\nroute {\n    route(check);\n}\n";

#[test]
fn modparam_value_position_offers_params_of_that_module() {
    let items = completions(&catalog(), DOC, r#"modparam("cachedb_nats", ""#);
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(names, vec!["kv_bucket"]);
    assert_eq!(items[0].kind, CompKind::Param);
}

#[test]
fn modparam_module_position_offers_module_names() {
    let items = completions(&catalog(), DOC, r#"modparam(""#);
    let names: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(names.contains(&"tm") && names.contains(&"cachedb_nats"));
    assert!(items.iter().all(|i| i.kind == CompKind::Module));
}

#[test]
fn loadmodule_position_offers_so_names() {
    let items = completions(&catalog(), DOC, r#"loadmodule ""#);
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"tm.so"));
    assert!(labels.contains(&"cachedb_nats.so"));
}

#[test]
fn code_position_offers_loaded_module_functions_and_routes() {
    let items = completions(&catalog(), DOC, "    t_re");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    // tm is loaded → t_relay offered; cachedb_nats is NOT loaded → not offered
    assert!(labels.contains(&"t_relay"));
    assert!(!labels.contains(&"nats_kv_get"));
    // route names as targets
    assert!(labels.contains(&"check"));
}

#[test]
fn hover_finds_function_param_and_module() {
    let h = hover_markdown(&catalog(), DOC, "t_relay").expect("function hover");
    assert!(h.contains("t_relay([flags],[outbound_proxy])"));
    assert!(h.contains("Relay statefully."));

    let h = hover_markdown(&catalog(), DOC, "kv_bucket").expect("param hover");
    assert!(h.contains("string") && h.contains("Bucket name."));

    let h = hover_markdown(&catalog(), DOC, "tm").expect("module hover");
    assert!(h.contains("tm"));

    assert!(hover_markdown(&catalog(), DOC, "no_such_thing").is_none());
}

#[test]
fn definition_resolves_route_reference() {
    // cursor on "check" inside route(check) on line 6
    let d = definition_of(DOC, 6, 11).expect("definition");
    assert_eq!(d.line, 2); // route[check] { on line 2
    // cursor elsewhere → none
    assert!(definition_of(DOC, 0, 0).is_none());
    // out of range must not panic
    assert!(definition_of(DOC, 999, 999).is_none());
}

#[test]
fn adversarial_docs_do_not_panic() {
    for text in ["", "\0", "route(", "modparam(\"x"] {
        let _ = completions(&catalog(), text, text);
        let _ = hover_markdown(&catalog(), text, "x");
        let _ = definition_of(text, 0, 0);
    }
}

fn core() -> opensips_lsp::catalog::CoreDocs {
    opensips_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "cache_store".into(),
            detail: "cache_store(storage_id, attribute, value, [timeout])".into(),
            doc: "Stores a value.".into(),
        }],
        params: vec![Item {
            name: "advertised_address".into(),
            detail: "core parameter".into(),
            doc: "Address advertised in Via.".into(),
        }],
        pvars: vec![Item {
            name: "$ru".into(),
            detail: "Request URI".into(),
            doc: "The full request URI.".into(),
        }],
    }
}

#[test]
fn code_position_offers_core_functions() {
    use opensips_lsp::logic::completions_with_core;
    let items = completions_with_core(&catalog(), &core(), DOC, "    cache_");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert!(labels.contains(&"cache_store"));
    assert!(labels.contains(&"t_relay")); // module functions still there
    assert!(labels.contains(&"advertised_address")); // core params too
}

#[test]
fn dollar_prefix_offers_pseudo_variables_only() {
    use opensips_lsp::logic::completions_with_core;
    let items = completions_with_core(&catalog(), &core(), DOC, "    xlog(\"$");
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, vec!["$ru"]);
}

#[test]
fn hover_covers_core_items() {
    use opensips_lsp::logic::hover_markdown_with_core;
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "cache_store").unwrap();
    assert!(h.contains("cache_store(storage_id"));
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "ru").unwrap();
    assert!(h.contains("Request URI"));
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "advertised_address").unwrap();
    assert!(h.contains("Via"));
    // module symbols still win where they exist
    let h = hover_markdown_with_core(&catalog(), &core(), DOC, "t_relay").unwrap();
    assert!(h.contains("module tm"));
}

#[test]
fn route_symbol_at_finds_refs_and_def_names() {
    let doc = "route {\n    route(to.b);\n}\nroute[to.b] {\n    exit;\n}\n";
    // on the ref name (line 1, "to.b" starts at byte col 10)
    assert_eq!(route_symbol_at(doc, 1, 10), Some("to.b".to_string()));
    assert_eq!(route_symbol_at(doc, 1, 13), Some("to.b".to_string()));
    // one past the name → no symbol
    assert_eq!(route_symbol_at(doc, 1, 14), None);
    // on the def name (line 3, "to.b" starts at byte col 6)
    assert_eq!(route_symbol_at(doc, 3, 6), Some("to.b".to_string()));
    // on the keyword, not the name
    assert_eq!(route_symbol_at(doc, 3, 0), None);
    // out of range must not panic
    assert_eq!(route_symbol_at(doc, 99, 99), None);
    assert_eq!(route_symbol_at("", 0, 0), None);
}

#[test]
fn route_occurrences_cover_defs_and_refs() {
    let doc = "route {\n    route(a);\n    route(\"a\");\n}\nroute[a] {\n    route(a);\n}\nroute[b] { exit; }\n";
    let occ = route_occurrences(doc, "a");
    // 3 refs + 1 def
    assert_eq!(occ.len(), 4);
    assert_eq!(occ.iter().filter(|(_, is_def)| *is_def).count(), 1);
    let def = occ.iter().find(|(_, d)| *d).unwrap();
    assert_eq!((def.0.line, def.0.col), (4, 6));
    // no occurrences of an unknown name
    assert!(route_occurrences(doc, "zz").is_empty());
    // adversarial: empty name never matches the main route
    assert!(route_occurrences(doc, "").is_empty());
}

#[test]
fn definition_resolves_dotted_route_names() {
    let doc = "route {\n    route(to.b);\n}\nroute[to.b] {\n    exit;\n}\n";
    // cursor on the dot: word_at-based matching used to split here
    let d = definition_of(doc, 1, 12).expect("definition of dotted name");
    assert_eq!(d.name, "to.b");
    assert_eq!(d.line, 3);
}

#[test]
fn valid_route_name_gate() {
    // rename writes names UNQUOTED, so the gate must match the
    // unquoted grammar: ID = [A-Za-z][A-Za-z0-9_]* or NUMBER
    assert!(valid_route_name("relay"));
    assert!(valid_route_name("Relay_2"));
    assert!(valid_route_name("42")); // numeric route names are legal
    // legal only QUOTED in the cfg → renaming to them breaks configs
    assert!(!valid_route_name("to.b_1:x-y"));
    assert!(!valid_route_name("a.b"));
    assert!(!valid_route_name("a-b"));
    assert!(!valid_route_name("a:b"));
    assert!(!valid_route_name("1foo"));
    assert!(!valid_route_name("_lead")); // ID starts with a letter
    assert!(!valid_route_name(""));
    assert!(!valid_route_name("has space"));
    assert!(!valid_route_name("quote\""));
    assert!(!valid_route_name("nul\0"));
    assert!(!valid_route_name("paren("));
}

fn sig_catalog() -> Vec<ModuleDoc> {
    vec![ModuleDoc {
        name: "tm".into(),
        params: vec![],
        functions: vec![Item {
            name: "t_relay".into(),
            detail: "t_relay([flags], [next_hop])".into(),
            doc: "Relays the transaction.".into(),
        }],
    }]
}

#[test]
fn signature_at_finds_active_parameter() {
    let core = opensips_lsp::catalog::CoreDocs::default();
    let doc = "loadmodule \"tm.so\"\nroute {\n}\n";
    // first argument
    let s = signature_at(&sig_catalog(), &core, doc, "    t_relay(").expect("sig");
    assert_eq!(s.0, "t_relay([flags], [next_hop])");
    assert_eq!(s.2, 0);
    // second argument (comma inside a string must not count)
    let s = signature_at(&sig_catalog(), &core, doc, r#"    t_relay("a,b", "#).expect("sig");
    assert_eq!(s.2, 1);
    // nested call: innermost unclosed wins
    let core_with_fn = opensips_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "xlog".into(),
            detail: "xlog([level], format)".into(),
            doc: String::new(),
        }],
        ..Default::default()
    };
    let s = signature_at(&sig_catalog(), &core_with_fn, doc, "    t_relay(xlog(").expect("sig");
    assert_eq!(s.0, "xlog([level], format)");
    // a CLOSED nested call pops back to the outer one
    let s = signature_at(
        &sig_catalog(),
        &core_with_fn,
        doc,
        "    t_relay(xlog(\"x\"), ",
    )
    .expect("sig");
    assert_eq!(s.0, "t_relay([flags], [next_hop])");
    assert_eq!(s.2, 1);
    // unknown function → none
    assert!(signature_at(&sig_catalog(), &core, doc, "    nope(").is_none());
    // adversarial: never panic
    for p in ["", "(", ")))((", "\"", "t_relay(\"\\", "#t_relay(", "\0("] {
        let _ = signature_at(&sig_catalog(), &core, doc, p);
    }
}

#[test]
fn completions_dedup_prefers_richer_items() {
    // "xlog" exists as a core KEYWORD and as a core function: one item
    // must survive, and it must be the function (it carries docs)
    let core = opensips_lsp::catalog::CoreDocs {
        functions: vec![Item {
            name: "xlog".into(),
            detail: "xlog([level], format)".into(),
            doc: "Logs.".into(),
        }],
        ..Default::default()
    };
    let out = completions_with_core(&[], &core, "route {\n}\n", "    ");
    let xlogs: Vec<_> = out.iter().filter(|c| c.label == "xlog").collect();
    assert_eq!(xlogs.len(), 1, "duplicate labels must collapse");
    assert_eq!(xlogs[0].kind, CompKind::Function);
}

#[test]
fn route_call_argument_completes_route_names() {
    let doc = "route[relay] {\n    exit;\n}\nroute {\n}\n";
    let out = completions_with_core(
        &[],
        &opensips_lsp::catalog::CoreDocs::default(),
        doc,
        "    route(",
    );
    assert!(!out.is_empty());
    assert!(
        out.iter().all(|c| c.kind == CompKind::Route),
        "inside route( only route names complete: {:?}",
        out.iter().map(|c| &c.label).collect::<Vec<_>>()
    );
    assert!(out.iter().any(|c| c.label == "relay"));
    // quoted form and partial names too
    let out = completions_with_core(
        &[],
        &opensips_lsp::catalog::CoreDocs::default(),
        doc,
        "    route(\"re",
    );
    assert!(out.iter().any(|c| c.label == "relay"));
}

#[test]
fn pvar_tail_reports_replacement_length() {
    // "$ru" → replace "$ru" (3 bytes)
    assert_eq!(pvar_tail("    $ru"), Some(3));
    assert_eq!(pvar_tail("$"), Some(1));
    assert_eq!(pvar_tail("xlog($si"), Some(3));
    // not a pvar context
    assert_eq!(pvar_tail("xlog("), None);
    assert_eq!(pvar_tail(""), None);
    // "$x y" — space breaks the tail
    assert_eq!(pvar_tail("$x y"), None);
}

#[test]
fn include_closure_follows_cycles_and_depth_safely() {
    use std::path::{Path, PathBuf};
    // a.cfg includes b.cfg; b.cfg includes a.cfg (cycle) and missing.cfg
    let loader = |p: &Path| -> Option<String> {
        match p.to_str()? {
            "/x/a.cfg" => Some("include_file \"b.cfg\"\nroute[a_r] { exit; }\n".into()),
            "/x/b.cfg" => {
                Some("include_file \"a.cfg\"\ninclude_file \"missing.cfg\"\nloadmodule \"tm.so\"\nroute[b_r] { exit; }\n".into())
            }
            _ => None,
        }
    };
    let root_text = loader(Path::new("/x/a.cfg")).unwrap();
    let files = include_closure(Path::new("/x/a.cfg"), &root_text, &loader);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.to_str().unwrap()).collect();
    assert_eq!(
        paths,
        vec!["/x/a.cfg", "/x/b.cfg"],
        "cycle visited once, missing skipped"
    );
    // multi-file views
    let mods = loaded_modules_multi(&files);
    assert_eq!(mods, vec!["tm"]);
    let defs = route_defs_multi(&files);
    let names: Vec<(&str, &str)> = defs
        .iter()
        .map(|(p, l)| (p.to_str().unwrap(), l.name.as_str()))
        .collect();
    assert!(names.contains(&("/x/a.cfg", "a_r")));
    assert!(names.contains(&("/x/b.cfg", "b_r")));
    // depth bomb: a chain of self-includes must terminate
    let bomb = |_: &Path| -> Option<String> { Some("include_file \"z.cfg\"\n".into()) };
    let files = include_closure(Path::new("/x/z.cfg"), "include_file \"z.cfg\"\n", &bomb);
    assert_eq!(files.len(), 1);
    let _ = PathBuf::new();
}

#[test]
fn analyzer_diagnostics_flag_undefined_and_duplicate_routes() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    // undefined route ref
    let text = "route {\n    route(nope);\n}\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("nope"));
    assert_eq!(ds[0].line, 1);
    assert!(ds[0].col_start < ds[0].col_end);
    // defined in an include → clean
    let loader2 = |p: &Path| -> Option<String> {
        (p.to_str() == Some("/x/inc.cfg")).then(|| "route[nope] { exit; }\n".to_string())
    };
    let text2 = "include_file \"inc.cfg\"\nroute {\n    route(nope);\n}\n";
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), text2, &loader2).is_empty());
    // duplicate definitions: the LATER one in this file is flagged
    let text3 = "route[dup] { exit; }\nroute[dup] { exit; }\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text3, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(ds[0].line, 1);
    assert!(ds[0].message.contains("dup"));
    // clean file → empty; adversarial → no panic
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), "route { exit; }\n", &loader).is_empty());
    for s in [
        "",
        "\0",
        "route(",
        "include_file \"\0\"",
        "route(x) route[x]{}",
    ] {
        let _ = analyzer_diagnostics(Path::new("/x/t.cfg"), s, &loader);
    }
}

#[test]
fn analyzer_understands_route_zero_and_kind_namespaces() {
    use std::path::Path;
    let loader = |_: &Path| -> Option<String> { None };
    // route(0) targets the anonymous main route — no warning
    let text = "route {\n    route(0);\n}\n";
    assert!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty(),
        "route(0) is the main route"
    );
    // ...but with NO main route it is undefined
    let text = "failure_route[f] {\n    route(0);\n}\n";
    assert_eq!(
        analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).len(),
        1
    );
    // a failure_route[x] does NOT satisfy route(x): separate namespace
    let text = "failure_route[x] {\n    exit;\n}\nroute {\n    route(x);\n}\n";
    let ds = analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader);
    assert_eq!(ds.len(), 1, "{ds:?}");
    // a route[x] does
    let text = "route[x] {\n    exit;\n}\nroute {\n    route(x);\n}\n";
    assert!(analyzer_diagnostics(Path::new("/x/t.cfg"), text, &loader).is_empty());
}

#[test]
fn core_keyword_seed_matches_opensips() {
    let out = completions(&[], "route {\n}\n", "    ");
    let labels: Vec<&str> = out.iter().map(|c| c.label.as_str()).collect();
    // real core tokens
    for kw in ["break", "exit", "drop", "async", "launch", "xlog"] {
        assert!(labels.contains(&kw), "core token {kw} missing");
    }
    // module functions must not be seeded as keywords: strlen does not
    // exist in OpenSIPS at all; send_reply is signaling's export;
    // subst is textops'
    for kw in ["strlen", "send_reply", "subst"] {
        assert!(!labels.contains(&kw), "{kw} wrongly seeded as core");
    }
}

#[test]
fn signature_params_split_is_depth_aware() {
    // real harvested signature from the json module
    let parts = split_params("json_link($json(dest_id), $json(source_id))");
    assert_eq!(parts, vec!["$json(dest_id)", "$json(source_id)"]);
    // plain
    assert_eq!(
        split_params("t_relay([flags], [next_hop])"),
        vec!["[flags]", "[next_hop]"]
    );
    // no params
    assert!(split_params("fix_nated_contact()").is_empty());
    // commas inside quotes stay put
    assert_eq!(split_params(r#"f("a,b", c)"#), vec![r#""a,b""#, "c"]);
    // adversarial: no panic
    for s in ["", "(", ")", "f(((", "f\0(a,b)"] {
        let _ = split_params(s);
    }
}

#[test]
fn foreign_diags_attach_to_the_include_directive() {
    use std::path::Path;
    let text = "include_file \"sub_bad.cfg\"\nroute {\n    exit;\n}\n";
    // error attributed to the include's (absolute) path → maps to the
    // include_file directive's line in the root
    let f = attribute_foreign_diag(
        Path::new("/x/main.cfg"),
        text,
        "/x/sub_bad.cfg",
        1,
        "syntax error",
    );
    assert_eq!(f.line, 0, "the include_file directive is on line 0");
    assert!(f.col_end > f.col_start);
    assert!(f.message.contains("sub_bad.cfg"), "{}", f.message);
    assert!(
        f.message.contains("line 2"),
        "1-based line in msg: {}",
        f.message
    );
    assert!(f.message.contains("syntax error"));
    // an error in a TRANSITIVE include (no matching directive here)
    // still lands visibly at the top of the root
    let f = attribute_foreign_diag(
        Path::new("/x/main.cfg"),
        text,
        "/x/deeper.cfg",
        4,
        "bad thing",
    );
    assert_eq!((f.line, f.col_start), (0, 0));
    assert!(f.message.contains("deeper.cfg") && f.message.contains("bad thing"));
    // adversarial: no panic
    for (df, t) in [("", ""), ("\0", "include_file \"\0\""), ("/x/a", "#")] {
        let _ = attribute_foreign_diag(Path::new("/x/m.cfg"), t, df, 0, "m");
    }
}

#[test]
fn quick_fixes_offer_loadmodule_and_route_stub() {
    // catalog: t_relay lives in tm
    let cat = sig_catalog();
    // unknown command → "load module tm" inserted after the last loadmodule
    let doc = "loadmodule \"proto_udp.so\"\nroute {\n    t_relay();\n}\n";
    let fixes = quick_fixes(&cat, doc, "unknown command <t_relay>, missing loadmodule?");
    assert_eq!(fixes.len(), 1, "{fixes:?}");
    assert!(fixes[0].title.contains("tm"));
    assert_eq!(
        (fixes[0].line, fixes[0].col),
        (1, 0),
        "after the last loadmodule"
    );
    assert_eq!(fixes[0].insert, "loadmodule \"tm.so\"\n");
    // module already loaded → no fix
    let doc2 = "loadmodule \"tm.so\"\nroute {\n    t_relay();\n}\n";
    assert!(quick_fixes(&cat, doc2, "unknown command <t_relay>, missing loadmodule?").is_empty());
    // no loadmodule lines at all → insert at the top
    let fixes = quick_fixes(
        &cat,
        "route {\n    t_relay();\n}\n",
        "unknown command <t_relay>, missing loadmodule?",
    );
    assert_eq!((fixes[0].line, fixes[0].col), (0, 0));
    // unknown function nobody exports → no fix
    assert!(quick_fixes(&cat, doc, "unknown command <nope_fn>, missing loadmodule?").is_empty());

    // undefined route → create a stub at end of file
    let doc3 = "route {\n    route(missing);\n}\n";
    let fixes = quick_fixes(
        &[],
        doc3,
        "route 'missing' is not defined here or in included files",
    );
    assert_eq!(fixes.len(), 1);
    assert!(fixes[0].title.contains("missing"));
    assert_eq!(fixes[0].line, 3, "appended at end of file");
    assert!(fixes[0].insert.contains("route[missing]"));
    assert!(fixes[0].insert.contains("exit;"));
    // adversarial messages → no panic, no fix
    for m in [
        "",
        "unknown command <>",
        "route '' is not defined",
        "unknown command <\0>",
    ] {
        let _ = quick_fixes(&cat, doc, m);
    }
}

#[test]
fn catalog_diagnostics_flag_undocumented_modparams() {
    let cat = catalog(); // tm has fr_timeout; cachedb_nats has kv_bucket
    // unknown param of a KNOWN module → warning at the param
    let text = "modparam(\"tm\", \"fr_timeot\", 5)\n";
    let ds = catalog_diagnostics(&cat, text);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("fr_timeot") && ds[0].message.contains("tm"));
    assert_eq!(ds[0].line, 0);
    assert!(ds[0].col_start < ds[0].col_end);
    // documented param → clean
    assert!(catalog_diagnostics(&cat, "modparam(\"tm\", \"fr_timeout\", 5)\n").is_empty());
    // UNKNOWN module → silent (the catalog may simply not cover it)
    assert!(catalog_diagnostics(&cat, "modparam(\"nope\", \"x\", 1)\n").is_empty());
    // empty catalog → silent everywhere
    assert!(catalog_diagnostics(&[], text).is_empty());
}

#[test]
fn semantic_spans_cover_routes_and_pvars() {
    let text = "route[relay] {\n    xlog(\"$ru\");\n    $var(x) = 1;\n    route(relay);\n}\n";
    let spans = semantic_spans(text);
    // route def name, pvar $ru is inside a STRING (still a pvar in
    // opensips — interpolated), $var(x), route ref name
    let routes: Vec<_> = spans
        .iter()
        .filter(|s| s.kind == SemKind::RouteName)
        .collect();
    assert_eq!(routes.len(), 2, "{spans:?}");
    assert_eq!((routes[0].line, routes[0].col, routes[0].len), (0, 6, 5));
    assert_eq!((routes[1].line, routes[1].col, routes[1].len), (3, 10, 5));
    let pvars: Vec<_> = spans.iter().filter(|s| s.kind == SemKind::Pvar).collect();
    assert!(pvars.iter().any(|p| p.line == 1), "$ru inside the string");
    assert!(pvars.iter().any(|p| p.line == 2 && p.len == 7), "$var(x)");
    // comments contribute nothing
    assert!(semantic_spans("# $ru route(x)\n").is_empty());
    // adversarial: no panic
    for s in ["", "$", "$(", "route[", "\0$ru"] {
        let _ = semantic_spans(s);
    }
}

#[test]
fn semantic_spans_exclude_comments_via_classifier() {
    // a '#' INSIDE a string does not comment out the rest of the line
    let spans = semantic_spans("xlog(\"call #1: $ru\");\n");
    assert!(
        spans.iter().any(|s| s.kind == SemKind::Pvar),
        "pvar after an in-string '#' must be tokenized: {spans:?}"
    );
    // same rule for single-quoted strings
    let spans = semantic_spans("xlog('#: $ru');\n");
    assert!(spans.iter().any(|s| s.kind == SemKind::Pvar), "{spans:?}");
    // block comments hide pvars — on one line...
    assert!(semantic_spans("/* $ru */\n").is_empty());
    // ...and across lines (no '#' anywhere near the pvar)
    assert!(semantic_spans("/*\n  $ru\n*/\n").is_empty());
    // code resumes after the block comment ends
    let spans = semantic_spans("/* x */ $rd;\n");
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!((spans[0].line, spans[0].col, spans[0].len), (0, 8, 3));
    // line comments still hide pvars; a pvar BEFORE the '#' stays
    let spans = semantic_spans("$ru; # $rd\n");
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(spans[0].col, 0);
    // adversarial: NUL, backslashes, unterminated constructs — no panic
    for s in [
        "\0/* $ru",
        "\"\\\\\" $ru # $rd",
        "'#' $ru",
        "/* $ru",
        "\"$ru",
    ] {
        let _ = semantic_spans(s);
    }
}

#[test]
fn semantic_tokens_delta_encoding() {
    let text = "route[ab] {\n    route(ab);\n}\n";
    let data = encode_semantic_tokens(text);
    // LSP quintuples: deltaLine, deltaStart, length, tokenType, mods
    assert_eq!(data.len() % 5, 0);
    assert!(!data.is_empty());
    // first token: line 0, col 6, len 2, type 0 (route name)
    assert_eq!(&data[..5], &[0, 6, 2, 0, 0]);
    // second token on line 1 → deltaLine 1, absolute col 10
    assert_eq!(&data[5..10], &[1, 10, 2, 0, 0]);
}

#[test]
fn route_families_are_separate_namespaces_for_navigation() {
    use opensips_lsp::logic::{RouteNs, ns_occurrences, route_symbol_ns_at};
    // failure_route[x] does NOT define a route() target: route(x)
    // invokes only the main table (route[x]); failure routes are
    // armed via t_on_failure("x")
    let doc = "failure_route[x] {\n    exit;\n}\nroute {\n    route(x);\n}\n";
    assert!(
        definition_of(doc, 4, 10).is_none(),
        "route(x) must not resolve to failure_route[x]"
    );
    // occurrences of main-ns "x" exclude the failure_route def
    let occ = route_occurrences(doc, "x");
    assert_eq!(occ.len(), 1, "call site only: {occ:?}");
    assert!(!occ[0].1, "no definition in the main namespace");
    // with a real route[x] present, both resolve within their tables
    let doc2 =
        "failure_route[x] {\n    exit;\n}\nroute[x] {\n    exit;\n}\nroute {\n    route(x);\n}\n";
    let d = definition_of(doc2, 7, 10).expect("resolves to route[x]");
    assert_eq!(d.line, 3, "route[x], not failure_route[x]");
    let occ = route_occurrences(doc2, "x");
    assert_eq!(occ.len(), 2, "call + main def only: {occ:?}");
    assert!(occ.iter().all(|(l, _)| l.line != 0), "failure def excluded");
    // the failure_route name is its own namespace
    let (n, ns) = route_symbol_ns_at(doc2, 0, 14).expect("symbol at failure name");
    assert_eq!(n, "x");
    assert_eq!(ns, RouteNs::Kind("failure_route".into()));
    let kocc = ns_occurrences(doc2, "x", &RouteNs::Kind("failure_route".into()));
    assert_eq!(kocc.len(), 1, "just the failure def: {kocc:?}");
    assert!(kocc[0].1);
    assert_eq!(kocc[0].0.line, 0);
    // main ns from a call site
    let (n, ns) = route_symbol_ns_at(doc2, 7, 10).expect("symbol at call");
    assert_eq!((n.as_str(), ns), ("x", RouteNs::Main));
    // adversarial: no panic
    for s in ["", "\0failure_route[x]{", "route("] {
        let _ = route_symbol_ns_at(s, 0, 0);
        let _ = ns_occurrences(s, "x", &RouteNs::Main);
    }
}

#[test]
fn semantic_tokens_range_encoding_edges() {
    use opensips_lsp::logic::encode_semantic_tokens_range;
    let text = "route[ab] {\n    route(ab);\n}\n";
    // whole-doc range == full encoding
    assert_eq!(
        encode_semantic_tokens_range(text, 0, 0, 99, 0),
        opensips_lsp::logic::encode_semantic_tokens(text)
    );
    // slice covering only line 1
    let d = encode_semantic_tokens_range(text, 1, 0, 2, 0);
    assert_eq!(d, vec![1, 10, 2, 0, 0]);
    // inverted / empty ranges yield nothing, no panic
    assert!(encode_semantic_tokens_range(text, 2, 0, 1, 0).is_empty());
    assert!(encode_semantic_tokens_range(text, 0, 0, 0, 0).is_empty());
    // boundary: span must lie FULLY inside (cut mid-name → excluded)
    assert!(encode_semantic_tokens_range(text, 1, 0, 1, 11).is_empty());
    for s in ["", "\0", "route["] {
        let _ = encode_semantic_tokens_range(s, 0, 0, 9, 9);
    }
}

#[test]
fn per_version_analysis_is_memoized() {
    use opensips_lsp::memo::AnalysisCache;
    let cache = AnalysisCache::default();
    let text = "route[a] { exit; }\nroute {\n    route(a);\n}\n";
    let a1 = cache.get_or_compute("file:///t.cfg", 1, text);
    let a2 = cache.get_or_compute("file:///t.cfg", 1, text);
    assert!(
        std::sync::Arc::ptr_eq(&a1, &a2),
        "same (uri, version) must reuse the computation"
    );
    assert_eq!(a1.blocks.len(), 2);
    assert_eq!(a1.refs.len(), 1);
    // a new version recomputes and evicts the old entry
    let a3 = cache.get_or_compute("file:///t.cfg", 2, "route { exit; }\n");
    assert!(!std::sync::Arc::ptr_eq(&a1, &a3));
    assert_eq!(a3.blocks.len(), 1);
    assert_eq!(cache.len(), 1, "one entry per document");
    // distinct documents don't collide
    let b = cache.get_or_compute("file:///u.cfg", 1, text);
    assert_eq!(b.blocks.len(), 2);
    assert_eq!(cache.len(), 2);
    // eviction on close
    cache.evict("file:///t.cfg");
    assert_eq!(cache.len(), 1);
    // adversarial: hostile text is just data
    let c = cache.get_or_compute("file:///v.cfg", 1, "\0route[\\ {");
    assert!(c.refs.is_empty());
}
