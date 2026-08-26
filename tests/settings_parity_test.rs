//! The three places a setting has to exist, held against each other.
//!
//! A setting lives in the server (read from `initializationOptions`),
//! in the extension manifest (so it can be set at all), and in the
//! client's restart list (so changing it takes effect). Existing gates
//! check each is DOCUMENTED. None checked they agree with each other,
//! and `versionInHints` shipped read-but-undocumented until a docs
//! gate caught it at test time — with nothing at all standing between
//! "the server reads it" and "the user can set it".

use std::collections::BTreeSet;

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{}/{}", env!("CARGO_MANIFEST_DIR"), rel))
        .unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Init options the server reads.
///
/// Every read goes through an `opts`/`s` map, and two `.get("...")`
/// calls in the file are not settings at all: `.get("route")` indexes
/// a JSON body, and `.get("opensipsLsp")` unwraps the namespace. A
/// first version of this gate swept both up and reported them as
/// unreachable settings — the gate being wrong, not the code — so
/// both are named here rather than filtered by a shape rule that
/// would quietly swallow a real setting too.
fn server_options() -> BTreeSet<String> {
    const NOT_SETTINGS: &[&str] = &["route", "opensipsLsp"];
    let src = read("src/server.rs");
    let mut out = BTreeSet::new();
    for (i, _) in src.match_indices(".get(\"") {
        if let Some((name, _)) = src[i + 6..].split_once('"')
            && !name.is_empty()
            && !NOT_SETTINGS.contains(&name)
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// How the client renames a manifest setting on its way to the
/// server: `analyzerDiagnostics: cfg.get('diagnostics.analyzer')`.
///
/// The two names differ on purpose — the manifest groups settings for
/// the settings UI — so comparing them directly is meaningless. This
/// reads the translation the client actually performs.
fn client_renames() -> Vec<(String, String)> {
    let ext = read("client/src/extension.ts");
    let mut out = Vec::new();
    for (i, _) in ext.match_indices(": cfg.get") {
        // the server-side name is the identifier before the colon
        let server = ext[..i]
            .rsplit(|c: char| c.is_whitespace() || c == '{' || c == ',')
            .next()
            .unwrap_or("")
            .to_string();
        // and the manifest name is the first quoted string after it,
        // which may be on a LATER LINE: this client wraps its long
        // calls, and a line-at-a-time reader saw 13 of 20 of them
        let rest = &ext[i..];
        let Some(q) = rest.find('\'') else { continue };
        let Some((manifest, _)) = rest[q + 1..].split_once('\'') else {
            continue;
        };
        if server.is_empty() || manifest.is_empty() {
            continue;
        }
        out.push((manifest.to_string(), server));
    }
    out
}

/// Settings the extension manifest contributes, without the namespace.
fn manifest_settings() -> BTreeSet<String> {
    let pkg: serde_json::Value = serde_json::from_str(&read("client/package.json")).unwrap();
    pkg["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("contributes.configuration.properties")
        .keys()
        .filter_map(|k| k.split_once('.').map(|(_, rest)| rest.to_string()))
        .collect()
}

/// Settings the client rebuilds the server for.
fn restart_settings() -> BTreeSet<String> {
    let ext = read("client/src/extension.ts");
    let block = ext
        .split_once("const restartSettings")
        .and_then(|(_, r)| r.split_once("];"))
        .map(|(b, _)| b)
        .expect("restartSettings exists");
    block
        .split('\'')
        .filter(|t| t.contains('.') && !t.contains('\n'))
        .filter_map(|t| t.split_once('.').map(|(_, rest)| rest.to_string()))
        .collect()
}

/// Settings whose VALUE the server reads at initialize only — as
/// opposed to the runtime toggles pushed by `didChangeConfiguration`.
fn initialize_only() -> BTreeSet<String> {
    let src = read("src/server.rs");
    let init = src
        .split_once("async fn initialize")
        .and_then(|(_, r)| r.split_once("Ok(InitializeResult"))
        .map(|(b, _)| b.to_string())
        .expect("initialize exists");
    let runtime = src
        .split_once("did_change_configuration")
        .map(|(_, r)| r.to_string())
        .unwrap_or_default();
    let named = |hay: &str| -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for (i, _) in hay.match_indices(".get(\"") {
            if let Some((n, _)) = hay[i + 6..].split_once('"') {
                out.insert(n.to_string());
            }
        }
        out
    };
    named(&init).difference(&named(&runtime)).cloned().collect()
}

#[test]
fn every_option_the_server_reads_can_be_set_in_the_editor() {
    let server = server_options();
    let manifest = manifest_settings();
    // POSITIVE CONTROL: scans that stopped matching would agree
    // trivially.
    assert!(
        server.len() >= 8,
        "only {} option(s) scanned: {server:?}",
        server.len()
    );
    assert!(
        manifest.len() >= 8,
        "only {} setting(s) in the manifest",
        manifest.len()
    );

    let renames = client_renames();
    assert!(
        renames.len() >= 4,
        "only {} rename(s) read from the client: {renames:?}",
        renames.len()
    );
    let reachable: BTreeSet<String> = manifest
        .iter()
        .cloned()
        .chain(renames.iter().map(|(_, server)| server.clone()))
        .collect();
    let unreachable: Vec<&String> = server.difference(&reachable).collect();
    assert!(
        unreachable.is_empty(),
        "the server reads these, and the extension offers no way to set them: {unreachable:?}"
    );
}

#[test]
fn the_manifest_offers_nothing_the_server_ignores() {
    let server = server_options();
    let manifest = manifest_settings();
    assert!(server.len() >= 8 && manifest.len() >= 8, "control");

    // `enable`, `serverPath` and `trace.server` are the client's own:
    // it acts on them before the server exists, so the server never
    // reads them. Every other manifest entry must reach the server.
    // The client acts on these itself and the server never sees them:
    // `enable` and `serverPath` decide whether and what to launch,
    // `trace.server` is the language-client's own tracing, and
    // `associateIncludedFiles` gates a decision the extension makes
    // about document languages. Each is verified as read in
    // `client/src/extension.ts` rather than assumed client-side
    // because the server does not read it — which is exactly what an
    // orphaned setting would look like.
    const CLIENT_ONLY: &[&str] = &[
        "enable",
        "serverPath",
        "trace.server",
        "diagnostics.enable",
        "associateIncludedFiles",
    ];
    let ext = read("client/src/extension.ts");
    for c in CLIENT_ONLY {
        if *c == "trace.server" {
            continue; // consumed by vscode-languageclient, not our code
        }
        assert!(
            ext.contains(&format!("'{c}'")),
            "{c} is excused as client-only but the client does not read it either — \
             that is an orphan, not an exception"
        );
    }
    let renamed: BTreeSet<String> = client_renames()
        .into_iter()
        .map(|(manifest, _)| manifest)
        .collect();
    let ignored: Vec<&String> = manifest
        .difference(&server)
        .filter(|s| !CLIENT_ONLY.contains(&s.as_str()) && !renamed.contains(*s))
        .collect();
    assert!(
        ignored.is_empty(),
        "the manifest offers these and nothing reads them: {ignored:?}"
    );
}

#[test]
fn a_setting_read_only_at_startup_restarts_the_server() {
    let init_only = initialize_only();
    let restarts = restart_settings();
    assert!(
        init_only.len() >= 4,
        "only {} initialize-only option(s): {init_only:?}",
        init_only.len()
    );

    let silent: Vec<&String> = init_only.difference(&restarts).collect();
    assert!(
        silent.is_empty(),
        "read once at initialize and NOT in the client's restart list, so changing them \
         is pushed to a server that never looks again — the setting appears to do nothing: \
         {silent:?}"
    );
}

#[test]
fn the_restart_list_names_no_setting_that_does_not_exist() {
    let restarts = restart_settings();
    let manifest = manifest_settings();
    assert!(
        restarts.len() >= 5,
        "only {} restart entry(s): {restarts:?}",
        restarts.len()
    );

    let ghosts: Vec<&String> = restarts
        .iter()
        .filter(|r| !r.is_empty() && !manifest.contains(*r))
        .collect();
    assert!(
        ghosts.is_empty(),
        "the restart list names settings the manifest does not offer — a rename that \
         missed one leaves the real setting silently non-restarting: {ghosts:?}"
    );
}

/// The option scan must see every read site.
///
/// This gate's first version bounded the scan to two handler bodies
/// and captured five options out of twelve, then reported the rest as
/// unreachable — the scan being wrong, not the code. A scan that
/// silently narrows turns every gate built on it into a gate that
/// passes for the wrong reason.
#[test]
fn the_option_scan_sees_every_read_site() {
    let src = read("src/server.rs");
    const NOT_SETTINGS: &[&str] = &["route", "opensipsLsp"];
    let sites = src.matches(".get(\"").count();
    let scanned = server_options().len();
    assert_eq!(
        scanned + NOT_SETTINGS.len(),
        sites,
        "the scan sees {scanned} of {sites} `.get(\"...\")` sites (excusing \
         {NOT_SETTINGS:?}) — it is missing read sites or inventing them"
    );
}

/// The rename map must be read from the client, not assumed.
#[test]
fn the_rename_map_covers_every_pushed_setting() {
    let ext = read("client/src/extension.ts");
    let pushed = ext.matches(": cfg.get").count();
    let mapped = client_renames().len();
    assert!(pushed >= 4, "control: {pushed} pushed setting(s)");
    assert_eq!(
        mapped, pushed,
        "the rename map reads {mapped} of {pushed} pushed settings — a setting it \
         misses looks like one the manifest offers and nothing reads"
    );
}

/// Every setting the docs describe must exist.
///
/// The existing gate runs the other way: settings the server reads
/// must be documented. A row for a setting that was renamed or
/// removed passes that gate and sends the reader to a setting that
/// does nothing.
#[test]
fn every_documented_setting_still_exists() {
    let features = read("docs/FEATURES.md");
    let manifest = manifest_settings();
    let mut ghosts: Vec<String> = Vec::new();
    for line in features
        .lines()
        .filter(|l| l.trim_start().starts_with("| `opensipsLsp."))
    {
        let Some(name) = line.split('`').nth(1) else {
            continue;
        };
        let Some((_, bare)) = name.split_once('.') else {
            continue;
        };
        if !manifest.contains(bare) {
            ghosts.push(name.to_string());
        }
    }
    assert!(
        ghosts.is_empty(),
        "documented settings that the extension no longer offers: {ghosts:?}"
    );
}

/// Every offered setting has a default and a description.
///
/// A setting with no default renders as an empty box the reader has
/// to guess at; one with no description is a name with nothing
/// explaining it — the same defect the keywords had.
#[test]
fn every_offered_setting_has_a_default_and_a_description() {
    let pkg: serde_json::Value = serde_json::from_str(&read("client/package.json")).unwrap();
    let props = pkg["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("properties");
    assert!(props.len() >= 8, "control: {} setting(s)", props.len());

    let mut bad: Vec<String> = Vec::new();
    for (name, spec) in props {
        if spec.get("default").is_none() {
            bad.push(format!("{name}: no default"));
        }
        if spec
            .get("description")
            .and_then(|d| d.as_str())
            .is_none_or(|d| d.trim().is_empty())
        {
            bad.push(format!("{name}: no description"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n  "));
}
