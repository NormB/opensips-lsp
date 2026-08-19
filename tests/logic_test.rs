use opensips_lsp::catalog::{Item, ModuleDoc};
use opensips_lsp::logic::{CompKind, completions, definition_of, hover_markdown};

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
