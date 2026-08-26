use opensips_lsp::catalog::{Item, ModuleDoc};
use opensips_lsp::logic::{
    CompKind, SemKind, analyzer_diagnostics, attribute_foreign_diag, catalog_diagnostics,
    completions, completions_with_core, configs_in_dir, definition_of, encode_semantic_tokens,
    hover_markdown, include_closure, loaded_modules_multi, pvar_tail, quick_fixes,
    route_defs_multi, route_occurrences, route_symbol_at, scan_configs, semantic_spans,
    signature_at, split_params, valid_route_name,
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
        statements: vec![Item {
            name: "if".into(),
            detail: "control statement".into(),
            doc: "Chooses between two paths.".into(),
        }],
        routes: vec![Item {
            name: "startup_route".into(),
            detail: "route type".into(),
            doc: "Runs once at startup.".into(),
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

/// Configs as the workspace scan hands them over.
fn ws(entries: &[(&str, &str)]) -> Vec<(std::path::PathBuf, String)> {
    entries
        .iter()
        .map(|(p, t)| (std::path::PathBuf::from(p), t.to_string()))
        .collect()
}

#[test]
fn the_include_graph_finds_the_root_a_fragment_belongs_to() {
    use opensips_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};
    // Transitivity is free: `sub/auth.cfg` is never named by
    // opensips.cfg, but routes.cfg is itself a scanned config and
    // names it, so walking parents reaches the top of the chain.
    let g = IncludeGraph::build(&ws(&[
        ("/w/opensips.cfg", "include_file \"routes.cfg\"\n"),
        ("/w/routes.cfg", "import_file \"sub/auth.cfg\"\n"),
        ("/w/sub/auth.cfg", "route[AUTH] { exit; }\n"),
        ("/w/other.cfg", "route { exit; }\n"),
    ]));
    assert_eq!(
        g.analysis_root(Path::new("/w/sub/auth.cfg")),
        Some(PathBuf::from("/w/opensips.cfg")),
        "a fragment two levels down belongs to the top of its chain"
    );
    assert_eq!(
        g.analysis_root(Path::new("/w/routes.cfg")),
        Some(PathBuf::from("/w/opensips.cfg"))
    );
    // a config nothing includes is a program in its own right
    assert_eq!(g.analysis_root(Path::new("/w/opensips.cfg")), None);
    assert_eq!(g.analysis_root(Path::new("/w/other.cfg")), None);
    // a file the scan never saw
    assert_eq!(g.analysis_root(Path::new("/w/nope.cfg")), None);
}

#[test]
fn the_include_graph_terminates_on_cycles_and_chooses_one_root_stably() {
    use opensips_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};
    // a <-> b: walking up from either must stop instead of looping
    let g = IncludeGraph::build(&ws(&[
        ("/w/a.cfg", "include_file \"b.cfg\"\n"),
        ("/w/b.cfg", "include_file \"a.cfg\"\n"),
    ]));
    let r = g.analysis_root(Path::new("/w/b.cfg"));
    assert!(r.is_some(), "a cycle still yields a decision: {r:?}");
    // self-include
    let g = IncludeGraph::build(&ws(&[("/w/z.cfg", "include_file \"z.cfg\"\n")]));
    assert_eq!(g.analysis_root(Path::new("/w/z.cfg")), None);
    // two roots include the same fragment: whichever is chosen, the
    // choice must not depend on scan order, or a fragment's context
    // would flicker between edits
    let fwd = IncludeGraph::build(&ws(&[
        ("/w/one.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/two.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/shared.cfg", "route[S] { exit; }\n"),
    ]));
    let rev = IncludeGraph::build(&ws(&[
        ("/w/shared.cfg", "route[S] { exit; }\n"),
        ("/w/two.cfg", "include_file \"shared.cfg\"\n"),
        ("/w/one.cfg", "include_file \"shared.cfg\"\n"),
    ]));
    let a = fwd.analysis_root(Path::new("/w/shared.cfg"));
    assert_eq!(a, rev.analysis_root(Path::new("/w/shared.cfg")));
    assert_eq!(a, Some(PathBuf::from("/w/one.cfg")));
    // adversarial: never panic
    for t in [
        "",
        "\0",
        "include_file",
        "include_file \"\0\"",
        "include_file \"..\"",
    ] {
        let g = IncludeGraph::build(&ws(&[("/w/x.cfg", t)]));
        let _ = g.analysis_root(Path::new("/w/x.cfg"));
    }
}

#[test]
fn a_fragment_is_analysed_in_its_roots_closure_not_on_its_own() {
    use opensips_lsp::logic::analyzer_diagnostics_in_closure;
    use std::path::Path;
    // opensips.cfg defines HELPER and includes inc.cfg, which calls
    // it.  Checked on its own inc.cfg looks broken; in the root's
    // closure it is not — the exact noise that made fragments
    // unusable.
    let main_text = "include_file \"inc.cfg\"\nroute[HELPER] { exit; }\n";
    let inc_text = "route {\n    route(HELPER);\n}\n";
    let loader = |p: &Path| -> Option<String> {
        match p.to_str()? {
            "/w/opensips.cfg" => Some(main_text.to_string()),
            "/w/inc.cfg" => Some(inc_text.to_string()),
            _ => None,
        }
    };
    let alone = include_closure(Path::new("/w/inc.cfg"), inc_text, &loader);
    let ds = analyzer_diagnostics_in_closure(&alone, Path::new("/w/inc.cfg"), inc_text);
    assert_eq!(ds.len(), 1, "on its own the call is undefined: {ds:?}");

    let via_root = include_closure(Path::new("/w/opensips.cfg"), main_text, &loader);
    let ds = analyzer_diagnostics_in_closure(&via_root, Path::new("/w/inc.cfg"), inc_text);
    assert!(ds.is_empty(), "in the root's closure it is defined: {ds:?}");
    // positions still belong to the reported file only
    let ds = analyzer_diagnostics_in_closure(&via_root, Path::new("/w/opensips.cfg"), main_text);
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn a_fragments_check_diagnostics_are_routed_to_the_fragment() {
    use opensips_lsp::diag::{Diag, Severity};
    use opensips_lsp::logic::fragment_check_diag;
    use std::path::Path;
    // `opensips -C` ran on the ROOT (cwd = its directory); the buffer
    // on screen is the fragment.
    let checked = Path::new("/w/opensips.cfg");
    let reported = Path::new("/w/incdir/sub.cfg");
    let mk = |file: &str| Diag {
        file: file.into(),
        line: 7,
        col_start: 4,
        col_end: 9,
        severity: Severity::Error,
        message: "syntax error".into(),
    };
    // relative, as the checker spells an include
    let d = fragment_check_diag(checked, reported, &mk("incdir/sub.cfg")).expect("fragment diag");
    assert_eq!(d.line, 7, "at the fragment's own line, not folded");
    assert_eq!(d.col_start, 4);
    assert_eq!(d.message, "syntax error", "no include-directive prefix");
    assert_eq!(d.file, reported.display().to_string());
    // absolute spelling of the same file
    assert!(fragment_check_diag(checked, reported, &mk("/w/incdir/sub.cfg")).is_some());
    // the root's own errors belong to the root's buffer, not here
    assert!(fragment_check_diag(checked, reported, &mk("/w/opensips.cfg")).is_none());
    // a sibling fragment's errors are not this fragment's
    assert!(fragment_check_diag(checked, reported, &mk("incdir/other.cfg")).is_none());
    // an unpositioned line must not silently become the fragment's
    assert!(fragment_check_diag(checked, reported, &mk("")).is_none());
    // adversarial: never panic
    for f in ["\0", "..", "a\\b", "incdir/../incdir/sub.cfg"] {
        let _ = fragment_check_diag(checked, reported, &mk(f));
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
    let origin = opensips_lsp::catalog::CatalogOrigin::BuiltIn("4.0.1".to_string());
    let ds = catalog_diagnostics(&cat, &origin, text);
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("fr_timeot") && ds[0].message.contains("tm"));
    assert_eq!(ds[0].line, 0);
    assert!(ds[0].col_start < ds[0].col_end);
    // documented param → clean
    assert!(catalog_diagnostics(&cat, &origin, "modparam(\"tm\", \"fr_timeout\", 5)\n").is_empty());
    // UNKNOWN module → silent (the catalog may simply not cover it)
    assert!(catalog_diagnostics(&cat, &origin, "modparam(\"nope\", \"x\", 1)\n").is_empty());
    // empty catalog → silent everywhere
    assert!(catalog_diagnostics(&[], &origin, text).is_empty());
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

#[test]
fn the_check_failure_note_never_invents_a_file_or_a_line() {
    use opensips_lsp::diag::{Diag, Severity};
    use opensips_lsp::logic::check_failure_note;
    let mk = |file: &str| Diag {
        file: file.into(),
        line: 0,
        col_start: 0,
        col_end: 1,
        severity: Severity::Error,
        message: "no transport protocol loaded".into(),
    };
    // positioned: the note says where, so the reader can go there
    let n = check_failure_note(Some(&mk("/w/root.cfg")), 1);
    assert!(n.contains("/w/root.cfg") && n.contains("line 1"), "{n}");
    // NOT positioned — a missing module, a bad module path.  Naming a
    // file and a line the parser never gave renders as
    // "check failed in , line 1: ..." and sends the reader to a line
    // that has nothing to do with it.
    let n = check_failure_note(Some(&mk("")), 1);
    assert!(!n.contains(" in ,"), "invented an empty file: {n}");
    assert!(!n.contains("line 1"), "invented a line: {n}");
    assert!(n.contains("no transport protocol loaded"), "{n}");
    // nothing parsed at all: the exit status is all there is
    let n = check_failure_note(None, 255);
    assert!(n.contains("255"), "{n}");
    assert!(!n.contains("line"), "{n}");
}

#[test]
fn the_include_graph_folds_a_path_that_climbs_out_of_its_directory() {
    use opensips_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};
    // A per-site split: `sites/site-a.cfg` reaches shared routing
    // through `../common/`, which is the only spelling the directive
    // CAN use.  The editor opens `/w/common/routing.cfg`; the
    // directive resolves to `/w/sites/../common/routing.cfg`.  One
    // file, two names — keyed apart, the fragment has no root and the
    // whole feature silently does nothing for that layout.
    let g = IncludeGraph::build(&ws(&[
        (
            "/w/sites/site-a.cfg",
            "include_file \"../common/routing.cfg\"\n",
        ),
        ("/w/common/routing.cfg", "route[R] { exit; }\n"),
    ]));
    assert_eq!(
        g.analysis_root(Path::new("/w/common/routing.cfg")),
        Some(PathBuf::from("/w/sites/site-a.cfg")),
        "a sibling directory reached through .. is the same file"
    );
    // `..` at the filesystem root is the root (POSIX), not a level above it
    let g = IncludeGraph::build(&ws(&[("/a.cfg", "include_file \"../x.cfg\"\n")]));
    assert_eq!(
        g.analysis_root(Path::new("/x.cfg")),
        Some(PathBuf::from("/a.cfg"))
    );
    // a `..` with nothing to fold into is kept rather than dropped,
    // or the path would name a different file entirely
    let g = IncludeGraph::build(&ws(&[("rel.cfg", "include_file \"../up.cfg\"\n")]));
    assert_eq!(
        g.analysis_root(Path::new("../up.cfg")),
        Some(PathBuf::from("rel.cfg"))
    );
}

#[test]
fn the_closure_visits_a_doubly_named_include_once() {
    use opensips_lsp::logic::include_closure;
    use std::path::Path;
    // The same file named twice — once directly, once through a
    // round trip — must not appear twice, or every route it defines
    // reads as "defined more than once".
    // the loader resolves BOTH spellings, as the filesystem does —
    // otherwise the second is skipped for being unloadable and the
    // test proves nothing
    let loader = |p: &Path| -> Option<String> {
        let resolved = p.to_str()?.replace("/inc/../inc/", "/inc/");
        match resolved.as_str() {
            "/w/opensips.cfg" => Some(
                "include_file \"inc/routes.cfg\"\ninclude_file \"inc/../inc/routes.cfg\"\n".into(),
            ),
            "/w/inc/routes.cfg" => Some("route[R] { exit; }\n".into()),
            _ => None,
        }
    };
    let root_text = loader(Path::new("/w/opensips.cfg")).unwrap();
    let files = include_closure(Path::new("/w/opensips.cfg"), &root_text, &loader);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.to_str().unwrap()).collect();
    assert_eq!(paths.len(), 2, "one root and one include: {paths:?}");
}

/// Randomised include graphs: the invariants, not the answers.
///
/// A hand-written fixture proves the cases its author thought of, and
/// every defect found in this feature so far lived in a case nobody
/// had thought of.  These build thousands of graphs from a fixed seed
/// — chains, diamonds, cycles, self-includes, orphans, files reached
/// by two spellings — and assert what has to hold for every one.
/// Fixed seed, so a failure is reproducible rather than a rumour.
#[test]
fn include_graph_invariants_hold_over_random_graphs() {
    use opensips_lsp::logic::IncludeGraph;
    use std::path::{Path, PathBuf};

    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for case in 0..3000u32 {
        let n = 2 + (rng() % 7) as usize;
        let names: Vec<String> = (0..n).map(|i| format!("/w/c{i}.cfg")).collect();
        let mut texts = vec![String::new(); n];
        for text in texts.iter_mut() {
            // each config includes a random subset of the others,
            // written in a random spelling of the same path
            for j in 0..n {
                if rng() % 3 != 0 {
                    continue;
                }
                let spelling = match rng() % 4 {
                    0 => format!("c{j}.cfg"),
                    1 => format!("./c{j}.cfg"),
                    2 => format!("../w/c{j}.cfg"),
                    _ => format!("/w/c{j}.cfg"),
                };
                text.push_str(&format!("include_file \"{spelling}\"\n"));
            }
        }
        let configs: Vec<(PathBuf, String)> = names
            .iter()
            .map(PathBuf::from)
            .zip(texts.iter().cloned())
            .collect();
        let g = IncludeGraph::build(&configs);
        // scan order must not decide anything: a fragment's context
        // cannot depend on which file the directory walk saw first
        let mut shuffled = configs.clone();
        shuffled.reverse();
        let g2 = IncludeGraph::build(&shuffled);

        for name in &names {
            let p = Path::new(name);
            // terminates (a hang fails the suite by timeout), and
            // never claims a file is its own root
            let root = g.analysis_root(p);
            assert_ne!(
                root.as_deref(),
                Some(p),
                "case {case}: {name} is its own root"
            );
            assert_eq!(
                root,
                g2.analysis_root(p),
                "case {case}: scan order changed the answer for {name}"
            );
            // whatever comes back must be reachable by walking
            // parents from the fragment — not merely plausible
            if let Some(r) = root {
                let mut cur = p.to_path_buf();
                let mut seen = std::collections::HashSet::new();
                let mut reached = false;
                while seen.insert(cur.clone()) {
                    match g.analysis_root(&cur) {
                        Some(up) => {
                            if up == r {
                                reached = true;
                                break;
                            }
                            cur = up;
                        }
                        None => break,
                    }
                }
                assert!(reached, "case {case}: {r:?} is not an ancestor of {name}");
            }
        }
    }
}

/// Every spelling of one path resolves to one key, and folding is
/// idempotent — otherwise "the same file" depends on how it was
/// written, which is how a fragment ends up with no root and a
/// closure that visits it twice.
#[test]
fn resolved_includes_fold_every_spelling_to_one_key() {
    use opensips_lsp::logic::resolved_includes;
    use std::path::{Component, Path};
    let from = Path::new("/w/sites/site.cfg");
    let spellings = [
        "../common/r.cfg",
        ".././common/r.cfg",
        "../common/./r.cfg",
        "../common/x/../r.cfg",
        "/w/common/r.cfg",
        "./../common/r.cfg",
    ];
    let text: String = spellings
        .iter()
        .map(|s| format!("include_file \"{s}\"\n"))
        .collect();
    let got = resolved_includes(from, &text);
    assert_eq!(got.len(), spellings.len(), "one entry per directive");
    for (spelling, path) in spellings.iter().zip(&got) {
        assert_eq!(
            path,
            Path::new("/w/common/r.cfg"),
            "{spelling} names the same file as the others"
        );
        // nothing foldable is left behind
        assert!(
            !path
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir)),
            "{path:?} still carries a . or .. component"
        );
    }
    // folding what is already folded changes nothing
    let again = resolved_includes(from, "include_file \"/w/common/r.cfg\"\n");
    assert_eq!(again, vec![std::path::PathBuf::from("/w/common/r.cfg")]);
}

/// Model-based: random operation sequences against an independent
/// model of the same question.
///
/// The graph is rebuilt from four places and read from many more.  A
/// stale answer is not an error — it is a correct-looking answer for
/// a workspace that no longer exists, which no single-step test can
/// see.  Here a plain model computes the root by climbing
/// first-parents, a sequence of random edits is applied to both, and
/// every file is compared after every step.
#[test]
fn the_graph_tracks_a_model_through_random_edits() {
    use opensips_lsp::logic::IncludeGraph;
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::path::{Path, PathBuf};

    /// The answer, computed the obvious slow way.
    fn model_root(inc: &BTreeMap<String, BTreeSet<String>>, f: &str) -> Option<String> {
        let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (src, tgts) in inc {
            for t in tgts {
                parents.entry(t.as_str()).or_default().push(src.as_str());
            }
        }
        for v in parents.values_mut() {
            v.sort();
        }
        let mut seen: HashSet<&str> = HashSet::new();
        seen.insert(f);
        let mut best: Option<String> = None;
        let mut cur = f;
        loop {
            let Some(next) = parents.get(cur).and_then(|v| v.first()).copied() else {
                return best;
            };
            if !seen.insert(next) {
                return best;
            }
            best = Some(next.to_string());
            cur = next;
        }
    }

    let names: Vec<String> = (0..6).map(|i| format!("/w/c{i}.cfg")).collect();
    let mut inc: BTreeMap<String, BTreeSet<String>> =
        names.iter().map(|n| (n.clone(), BTreeSet::new())).collect();

    let mut state = 0xDEAD_BEEF_CAFE_F00Du64;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for step in 0..600u32 {
        let f = names[(rng() % names.len() as u64) as usize].clone();
        let t = names[(rng() % names.len() as u64) as usize].clone();
        if rng() % 2 == 0 {
            if f != t {
                inc.get_mut(&f).unwrap().insert(t);
            }
        } else {
            let victim = inc[&f].iter().next().cloned();
            if let Some(v) = victim {
                inc.get_mut(&f).unwrap().remove(&v);
            }
        }
        let configs: Vec<(PathBuf, String)> = names
            .iter()
            .map(|n| {
                let body: String = inc[n]
                    .iter()
                    // every spelling of the same target, in turn
                    .enumerate()
                    .map(|(k, t)| match k % 3 {
                        0 => format!("include_file \"{t}\"\n"),
                        1 => format!("include_file \"{}\"\n", t.replace("/w/", "./")),
                        _ => format!("include_file \"{}\"\n", t.replace("/w/", "../w/")),
                    })
                    .collect();
                (PathBuf::from(n), body)
            })
            .collect();
        let g = IncludeGraph::build(&configs);
        for n in &names {
            assert_eq!(
                g.analysis_root(Path::new(n))
                    .map(|p| p.display().to_string()),
                model_root(&inc, n),
                "step {step}: {n} disagrees with the model; includes = {inc:?}"
            );
        }
    }
}

/// The workspace sweep must reach a configuration that is not named
/// `*.cfg`.
///
/// A tree whose root is `proxy.inc` — or an `.m4` template — was
/// invisible to the sweep, so no fragment under it ever resolved a
/// root and every one of them was analysed alone.  The sweep still
/// must not read the whole tree: it looks for configurations, not for
/// every file in the folder.
#[test]
fn the_workspace_sweep_reaches_configs_not_named_cfg() {
    let dir = std::env::temp_dir().join(format!("oslsp-sweep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("include")).unwrap();
    std::fs::write(
        dir.join("proxy.inc"),
        "include_file \"include/routes.inc\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("opensips.m4"),
        "include_file \"include/routes.inc\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("include/routes.inc"), "route { exit; }\n").unwrap();
    std::fs::write(dir.join("notes.md"), "not a config\n").unwrap();

    let (found, _) = scan_configs(std::slice::from_ref(&dir), 500);
    let names: Vec<String> = found
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    for want in ["proxy.inc", "opensips.m4", "routes.inc"] {
        assert!(
            names.contains(&want.to_string()),
            "{want} missing: {names:?}"
        );
    }
    assert!(
        !names.contains(&"notes.md".to_string()),
        "the sweep must not collect every file: {names:?}"
    );

    let here: Vec<String> = configs_in_dir(&dir)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert!(here.contains(&"proxy.inc".to_string()), "{here:?}");
    assert!(!here.contains(&"notes.md".to_string()), "{here:?}");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// A module whose parameters were never harvested documents nothing,
/// and an empty list is not evidence that a parameter does not exist.
///
/// `auth_web3` writes its README with `## Parameters` and `### name`
/// rather than the `### Exported Parameters` / `#### name` every
/// other module uses, so nothing of it is harvested but the name.
/// Once the module itself was in the catalogue — it is a module a
/// config can load — every `modparam` for it would have been reported
/// as undocumented, turning a silence into ten false warnings.
#[test]
fn a_module_documenting_no_parameters_at_all_stays_silent() {
    let cat = vec![ModuleDoc {
        name: "auth_web3".into(),
        params: Vec::new(),
        functions: Vec::new(),
    }];
    let text = "modparam(\"auth_web3\", \"authentication_rpc_url\", \"https://x\")\n";
    let origin = opensips_lsp::catalog::CatalogOrigin::BuiltIn("4.0.1".to_string());
    assert!(
        catalog_diagnostics(&cat, &origin, text).is_empty(),
        "an unharvested parameter list must not accuse the config"
    );

    // anything harvested makes the list evidence again
    let mut cat = cat;
    cat[0].params.push(Item {
        name: "rpc_timeout".into(),
        detail: "integer".into(),
        doc: String::new(),
    });
    assert_eq!(
        catalog_diagnostics(&cat, &origin, text).len(),
        1,
        "a module that does document parameters is still checked"
    );

    // and a module that exports functions but no parameters really
    // exports none — `sdpops` is that, and setting one is an error
    let functions_only = vec![ModuleDoc {
        name: "sipmsgops".into(),
        params: Vec::new(),
        functions: vec![Item {
            name: "is_method".into(),
            detail: String::new(),
            doc: String::new(),
        }],
    }];
    assert_eq!(
        catalog_diagnostics(
            &functions_only,
            &origin,
            "modparam(\"sipmsgops\", \"nope\", 1)\n"
        )
        .len(),
        1,
        "a module with functions but no parameters was read, and exports none"
    );
}
