//! What rename, references and highlights treat as an occurrence of a
//! route name, and when a rename must be refused.
//!
//! Ground truth, OpenSIPS 4.0.2:
//!
//! - `t_on_failure`, `t_on_reply` and `t_on_branch` take one
//!   `CMD_PARAM_STR` whose fixup resolves it as a failure, onreply or
//!   branch route name (`modules/tm/tm.c`, `fixup_froute`,
//!   `fixup_rroute`, `fixup_broute`). `opensips -C` accepts the name
//!   double- or single-quoted and rejects it bare (`syntax error`).
//! - `async(func(...), NAME [, NUMBER])` and
//!   `launch(func(...), NAME [, STRING])` resolve NAME in the REQUEST
//!   route table (`cfg.y`, `ASYNC_TOKEN` / `LAUNCH_TOKEN` rules, both
//!   `ref_script_route_by_name(..., sroutes->request, ...)`); NAME is a
//!   `route_name`: an ID, a NUMBER or a STRING.

use opensips_lsp::analyze::Located;
use opensips_lsp::logic::RouteNs;
use opensips_lsp::{analyze, logic};
use std::path::PathBuf;

const ARMED: &str = r#"route {
    t_on_failure("fr");
    t_on_failure('fr');
    t_on_failure("$var(r)");
    xlog("fr");
    # t_on_failure("fr");
    t_on_reply("fr");
    t_on_branch("br");
    t_relay();
}
failure_route[fr] {
    exit;
}
onreply_route[fr] {
    exit;
}
branch_route[br] {
    exit;
}
"#;

fn spots(occ: &[(Located, bool)]) -> Vec<(u32, u32, bool)> {
    let mut v: Vec<_> = occ.iter().map(|(l, d)| (l.line, l.col, *d)).collect();
    v.sort();
    v
}

#[test]
fn a_failure_route_is_named_by_the_strings_that_arm_it() {
    let occ = logic::ns_occurrences(ARMED, "fr", &RouteNs::Kind("failure_route".into()));
    assert_eq!(
        spots(&occ),
        vec![
            (1, 18, false), // t_on_failure("fr")
            (2, 18, false), // t_on_failure('fr')
            (10, 14, true), // failure_route[fr]
        ],
        "a pvar string, an xlog string, a comment and t_on_reply's \
         \"fr\" are not failure-route references"
    );
}

#[test]
fn reply_and_branch_routes_are_armed_by_their_own_functions() {
    let reply = logic::ns_occurrences(ARMED, "fr", &RouteNs::Kind("onreply_route".into()));
    assert_eq!(spots(&reply), vec![(6, 16, false), (13, 14, true)]);
    let branch = logic::ns_occurrences(ARMED, "br", &RouteNs::Kind("branch_route".into()));
    assert_eq!(spots(&branch), vec![(7, 17, false), (16, 13, true)]);
}

#[test]
fn the_cursor_on_an_arming_string_names_that_routes_namespace() {
    // inside the quotes of t_on_failure("fr")
    assert_eq!(
        logic::route_symbol_ns_at(ARMED, 1, 19),
        Some(("fr".into(), RouteNs::Kind("failure_route".into())))
    );
    assert_eq!(
        logic::route_symbol_ns_at(ARMED, 6, 17),
        Some(("fr".into(), RouteNs::Kind("onreply_route".into())))
    );
    // the pvar string is not a name
    assert_eq!(logic::route_symbol_ns_at(ARMED, 3, 20), None);
}

const RESUME: &str = r#"route {
    async(sleep(1), resume);
    async(sleep(1), "resume", 5);
    launch(sleep(1), resume, "x");
    launch(sleep(1));
    exit;
}
route[resume] {
    exit;
}
"#;

#[test]
fn async_and_launch_name_a_request_route_in_their_second_argument() {
    let refs: Vec<(String, u32, u32)> = analyze::route_refs(RESUME)
        .into_iter()
        .map(|r| (r.name, r.line, r.col))
        .collect();
    assert_eq!(
        refs,
        vec![
            ("resume".into(), 1, 20),
            ("resume".into(), 2, 21),
            ("resume".into(), 3, 21),
        ]
    );
    let occ = logic::ns_occurrences(RESUME, "resume", &RouteNs::Main);
    assert_eq!(occ.len(), 4, "three call sites and the definition");
}

#[test]
fn a_rename_onto_an_existing_route_in_the_same_namespace_conflicts() {
    let files = vec![
        (
            PathBuf::from("/a.cfg"),
            "route[taken] {\n    exit;\n}\n".to_string(),
        ),
        (
            PathBuf::from("/b.cfg"),
            "route[other] {\n    exit;\n}\nfailure_route[free] {\n    exit;\n}\n".to_string(),
        ),
    ];
    let hit = logic::rename_conflict(&files, "taken", &RouteNs::Main).expect("taken is defined");
    assert_eq!(hit.0, PathBuf::from("/a.cfg"));
    assert_eq!((hit.1.line, hit.1.col), (0, 6));
    // another namespace's name is not a conflict
    assert!(
        logic::rename_conflict(&files, "free", &RouteNs::Main).is_none(),
        "free is only a failure_route, so it does not conflict in the main namespace: {files:?}"
    );
    assert!(
        logic::rename_conflict(&files, "taken", &RouteNs::Kind("failure_route".into())).is_none(),
        "taken is only a request route, so it does not conflict among failure_route names: {files:?}"
    );
    assert!(
        logic::rename_conflict(&files, "free", &RouteNs::Kind("failure_route".into())).is_some(),
        "failure_route[free] is defined in /b.cfg, so renaming onto it conflicts: {files:?}"
    );
    // a name that is only CALLED is not defined, so it is free
    let called = vec![(
        PathBuf::from("/c.cfg"),
        "route {\n    route(ghost);\n}\n".to_string(),
    )];
    assert!(
        logic::rename_conflict(&called, "ghost", &RouteNs::Main).is_none(),
        "ghost is only called, never defined, so the name is free: {called:?}"
    );
}
