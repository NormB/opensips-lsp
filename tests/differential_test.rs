//! Differential gates against the REAL parser (the ground truth):
//!
//!   - the analyzer must be SILENT on every corpus config the binary
//!     accepts — any false positive on real-world configs is a CI
//!     failure, not an audit finding;
//!   - a rename applied through the real logic path must produce a
//!     config the binary still accepts.
//!
//! Both are env-gated: OPENSIPS_LSP_TEST_TREE / OPENSIPS_LSP_TEST_BIN.

mod common;

use opensips_lsp::logic;

/// Same enumeration as the corpus sweep: shipped example configs.
fn corpus(tree: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for dir in [tree.join("etc"), tree.join("examples")] {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "cfg") {
                out.push(p);
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(tree.join("modules")) {
        for e in rd.flatten() {
            for sub in ["examples", "etc"] {
                let d = e.path().join(sub);
                let Ok(rd2) = std::fs::read_dir(&d) else {
                    continue;
                };
                for f in rd2.flatten() {
                    let p = f.path();
                    if p.extension().is_some_and(|x| x == "cfg") {
                        out.push(p);
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn disk_loader(p: &std::path::Path) -> Option<String> {
    let md = std::fs::metadata(p).ok()?;
    if !md.is_file() || md.len() > 1_048_576 {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

#[test]
fn analyzer_is_silent_on_binary_accepted_configs() {
    let (tree, bin) = (
        common::required_env("OPENSIPS_LSP_TEST_TREE"),
        common::required_env("OPENSIPS_LSP_TEST_BIN"),
    );
    let mut files = corpus(std::path::Path::new(&tree));
    assert!(!files.is_empty(), "no corpus configs under {tree}");
    // the shipped examples may all be rejected on a box with a partial
    // module install; a synthetic baseline (verified-accepted
    // constructs, including past false-positive shapes: route(0),
    // include-defined targets, numeric routes) keeps the gate
    // non-vacuous everywhere
    let base = std::env::temp_dir().join(format!("oslsp-diff-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::fs::write(
        base.join("inc.cfg"),
        "route[from_include] {\n    exit;\n}\n",
    )
    .unwrap();
    for (name, text) in [
        (
            "baseline-basic.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[named] {\n    exit;\n}\nroute[42] {\n    exit;\n}\nroute {\n    route(named);\n    route(42);\n    route(0);\n    exit;\n}\n",
        ),
        (
            "baseline-include.cfg",
            "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\ninclude_file \"inc.cfg\"\nroute {\n    route(from_include);\n    exit;\n}\n",
        ),
    ] {
        let p = base.join(name);
        std::fs::write(&p, text).unwrap();
        files.push(p);
    }
    let mut accepted = 0usize;
    for f in &files {
        let Ok(out) = std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(f)
            .current_dir(f.parent().unwrap())
            .output()
        else {
            continue;
        };
        if out.status.code() != Some(0) {
            continue; // rejected configs are the corpus sweep's business
        }
        accepted += 1;
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let warns = logic::analyzer_diagnostics(f, &text, &disk_loader);
        assert!(
            warns.is_empty(),
            "{}: accepted by the real parser but the analyzer warns — \
             false positive:\n{warns:#?}",
            f.display()
        );
    }
    let _ = std::fs::remove_dir_all(&base);
    assert!(accepted > 0, "no config was accepted; gate is vacuous");
    eprintln!("analyzer silent on {accepted} binary-accepted configs");
}

#[test]
fn rename_round_trip_survives_the_real_parser() {
    let bin = common::required_env("OPENSIPS_LSP_TEST_BIN");
    let dir = std::env::temp_dir().join(format!("oslsp-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("rt.cfg");
    let original = "socket=udp:127.0.0.1:15999\nloadmodule \"proto_udp.so\"\nroute[old_target] {\n    exit;\n}\nroute {\n    route(old_target);\n    exit;\n}\n";
    std::fs::write(&cfg, original).unwrap();
    let check = |p: &std::path::Path| {
        std::process::Command::new(&bin)
            .arg("-C")
            .arg("-f")
            .arg(p)
            .current_dir(p.parent().unwrap())
            .output()
            .map(|o| o.status.code() == Some(0))
            .unwrap_or(false)
    };
    // a binary that rejects the baseline makes the round-trip prove
    // nothing, so it is a failure rather than a reason to opt out
    assert!(
        check(&cfg),
        "the test binary rejects the baseline config, so the rename \
         round-trip would prove nothing"
    );
    // rename through the REAL logic path: gate + occurrences
    let new_name = "relay_v2";
    assert!(logic::valid_route_name(new_name));
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let mut occ = logic::route_occurrences(original, "old_target");
    assert_eq!(occ.len(), 2, "def + one call site");
    // splice right-to-left so earlier columns stay valid
    occ.sort_by_key(|(l, _)| (l.line, std::cmp::Reverse(l.col)));
    for (l, _) in &occ {
        let line = &mut lines[l.line as usize];
        let (s, e) = (l.col as usize, l.col as usize + l.name.len());
        line.replace_range(s..e, new_name);
    }
    let renamed = lines.join("\n") + "\n";
    assert!(!renamed.contains("old_target"));
    assert!(renamed.matches(new_name).count() == 2);
    let out_cfg = dir.join("rt-renamed.cfg");
    std::fs::write(&out_cfg, &renamed).unwrap();
    assert!(
        check(&out_cfg),
        "the real parser rejects the renamed config:\n{renamed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The README's own `modparam` examples, as ground truth for what the
/// catalogue must contain.
///
/// The harvester reads headings; the examples underneath them are
/// written by the same author for the same release, and they name the
/// parameter the way a configuration has to write it.  Where the two
/// disagree the harvester is normally the one that is wrong — that is
/// how the fenced-example truncation, the `#####` sub-heading, the
/// unspaced `db_url(str)` and the comma-listed heading were each
/// found, and every one of them made the server warn about a
/// parameter that exists.
///
/// The exceptions below are the other direction: places where the
/// EXAMPLE is wrong, or where upstream documents no heading at all.
/// Each is a finding about OpenSIPS's documentation, not about this
/// parser, so each is listed by name rather than waved through by a
/// count.
const UPSTREAM_DOC_GAPS: [(&str, &str, &str); 32] = [
    ("auth_aka", "pending_timeout", "no heading documents it"),
    (
        "auth_jwt",
        "start_ts",
        "heading documents `start_ts_column`",
    ),
    ("auth_jwt", "end_ts", "heading documents `end_ts_column`"),
    // `## Parameters` with `### name`, not the `### Exported
    // Parameters` / `#### name` every other module writes
    (
        "auth_web3",
        "web3_authentication_rpc_url",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "web3_authentication_contract_address",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "web3_ens_rpc_url",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "web3_ens_registry_address",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "web3_contract_debug_mode",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "authentication_rpc_url",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "authentication_contract_address",
        "README shape not harvested",
    ),
    ("auth_web3", "ens_rpc_url", "README shape not harvested"),
    (
        "auth_web3",
        "ens_registry_address",
        "README shape not harvested",
    ),
    (
        "auth_web3",
        "contract_debug_mode",
        "README shape not harvested",
    ),
    ("auth_web3", "rpc_timeout", "README shape not harvested"),
    // headings write the index as a template: `app[index]_..._column`
    (
        "b2b_sca",
        "app1_shared_entity_column",
        "heading is `app[index]_shared_entity_column`",
    ),
    (
        "b2b_sca",
        "app2_shared_entity_column",
        "heading is `app[index]_shared_entity_column`",
    ),
    (
        "b2b_sca",
        "app1_call_state_column",
        "heading is `app[index]_call_state_column`",
    ),
    (
        "b2b_sca",
        "app2_call_state_column",
        "heading is `app[index]_call_state_column`",
    ),
    (
        "b2b_sca",
        "app1_call_info_uri_column",
        "heading is `app[index]_call_info_uri_column`",
    ),
    (
        "b2b_sca",
        "app2_call_info_uri_column",
        "heading is `app[index]_call_info_uri_column`",
    ),
    (
        "b2b_sca",
        "app1_call_info_appearance_uri_column",
        "heading is `app[index]_...`",
    ),
    (
        "b2b_sca",
        "app2_call_info_appearance_uri_column",
        "heading is `app[index]_...`",
    ),
    (
        "b2b_sca",
        "app1_b2bl_key_column",
        "heading is `appindex_b2bl_key_column`",
    ),
    (
        "b2b_sca",
        "app2_b2bl_key_column",
        "heading is `appindex_b2bl_key_column`",
    ),
    (
        "config",
        "restart_persistent_memory",
        "heading documents `enable_restart_persistency`",
    ),
    ("cpl_c", "cpl_table", "no heading documents it"),
    (
        "osp",
        "use_number_portablity",
        "example typo for `use_number_portability`",
    ),
    (
        "osp",
        "networkid_param",
        "heading documents `networkid_parameter`",
    ),
    (
        "osp",
        "switchid_param",
        "heading documents `switchid_parameter`",
    ),
    (
        "proto_bin",
        "tcp_async_local_write_timeout",
        "no heading documents it",
    ),
    (
        "rtpengine",
        "extra_failover_error",
        "no heading documents it",
    ),
    ("tcp_mgm", "connect_timeout_col", "no heading documents it"),
];

/// Every parameter a module's own README sets in an example is in the
/// catalogue.
#[test]
fn the_catalogue_contains_every_parameter_the_readmes_set() {
    let tree = common::required_env("OPENSIPS_LSP_TEST_TREE");
    let modules = std::path::Path::new(&tree).join("modules");
    let re =
        regex::Regex::new(r#"modparam\(\s*"([A-Za-z0-9_]+)"\s*,\s*"([A-Za-z0-9_]+)""#).unwrap();
    let catalogue = opensips_lsp::catalog::builtin_modules();

    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut stale: Vec<String> = Vec::new();
    let mut seen_gap: Vec<(&str, &str)> = Vec::new();
    let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&modules)
        .expect("an OpenSIPS source tree has modules/")
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let module = dir.file_name().unwrap().to_string_lossy().into_owned();
        let Ok(readme) = std::fs::read_to_string(dir.join("README.md")) else {
            continue;
        };
        let Some(doc) = catalogue.modules.iter().find(|m| m.name == module) else {
            missing.push(format!(
                "{module}: the module itself is not in the catalogue"
            ));
            continue;
        };
        for c in re.captures_iter(&readme) {
            // only what the module says about ITSELF: a README often
            // shows another module's parameter in a worked example
            if c[1] != module {
                continue;
            }
            let param = c[2].to_string();
            checked += 1;
            let excused = UPSTREAM_DOC_GAPS
                .iter()
                .find(|(m, p, _)| *m == module && *p == param);
            if doc.params.iter().any(|p| p.name == param) {
                if let Some((_, _, why)) = excused {
                    stale.push(format!(
                        "{module}::{param} is harvested now — drop it ({why})"
                    ));
                }
                continue;
            }
            match excused {
                Some((m, p, _)) => {
                    if !seen_gap.contains(&(m, p)) {
                        seen_gap.push((m, p));
                    }
                }
                None => missing.push(format!("{module}::{param}")),
            }
        }
    }
    assert!(checked > 1500, "suspiciously few examples read: {checked}");
    assert!(
        missing.is_empty(),
        "{} parameter(s) a README sets are not in the catalogue:\n{}",
        missing.len(),
        missing.join("\n")
    );
    // an exception that no longer fires is an exception that stopped
    // describing the tree — it hides the next regression
    assert!(stale.is_empty(), "stale exceptions:\n{}", stale.join("\n"));
    assert_eq!(
        seen_gap.len(),
        UPSTREAM_DOC_GAPS.len(),
        "exceptions that never fired: {:?}",
        UPSTREAM_DOC_GAPS
            .iter()
            .filter(|(m, p, _)| !seen_gap.contains(&(*m, *p)))
            .collect::<Vec<_>>()
    );
    eprintln!("catalogue covers {checked} README modparam examples");
}
