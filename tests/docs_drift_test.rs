//! Documentation drift gate.
//!
//! The docs are the product for everyone who has not read
//! `src/server.rs`, so a page that undersells a shipped capability is
//! a defect exactly like a page that promises a missing one.  This
//! gate derives the truth from the source — the capabilities
//! advertised in `initialize`, the options the server reads, the
//! settings the client contributes — and fails when a page does not
//! mention it.

use std::collections::BTreeSet;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{}/{rel}", root())).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every top-level capability advertised in `initialize`.  Fields of
/// nested option structs (`prepare_provider`, `resolve_provider`) are
/// indented deeper and are excluded by the indent test — they get
/// their own gate below.
fn advertised_providers() -> BTreeSet<String> {
    read("src/server.rs")
        .lines()
        .filter_map(|l| {
            let indent = l.len() - l.trim_start().len();
            let t = l.trim_start();
            (indent == 16 && t.contains("_provider: Some("))
                .then(|| t.split(':').next().unwrap().to_string())
        })
        .collect()
}

/// One documentation requirement per advertised capability: a phrase
/// that must appear in BOTH `README.md` and `docs/FEATURES.md`.  The
/// coverage assertion below forces a new capability to add its row
/// here, so this table cannot quietly fall behind the server.
const DOC_TOKENS: &[(&str, &str)] = &[
    ("call_hierarchy_provider", "call hierarchy"),
    ("code_action_provider", "quick fix"),
    ("code_lens_provider", "code lens"),
    ("completion_provider", "completion"),
    ("definition_provider", "go to definition"),
    ("diagnostic_provider", "pull diagnostic"),
    ("document_formatting_provider", "format"),
    ("document_highlight_provider", "highlight"),
    ("document_range_formatting_provider", "range formatting"),
    ("document_link_provider", "include link"),
    ("document_symbol_provider", "document symbol"),
    ("folding_range_provider", "fold"),
    ("hover_provider", "hover"),
    ("inlay_hint_provider", "inlay hint"),
    ("references_provider", "reference"),
    ("rename_provider", "rename"),
    ("semantic_tokens_provider", "semantic"),
    ("signature_help_provider", "signature help"),
    ("workspace_symbol_provider", "workspace symbol"),
];

#[test]
fn every_advertised_capability_is_documented() {
    let providers = advertised_providers();
    assert!(
        providers.len() >= 12,
        "only {} providers found — the capability block moved and this gate went blind",
        providers.len()
    );
    let mapped: BTreeSet<String> = DOC_TOKENS.iter().map(|(p, _)| (*p).to_string()).collect();
    assert_eq!(
        providers, mapped,
        "the advertised capabilities and the documentation map disagree"
    );

    let readme = read("README.md").to_lowercase();
    let features = read("docs/FEATURES.md").to_lowercase();
    let mut missing = Vec::new();
    for (provider, token) in DOC_TOKENS {
        if !readme.contains(*token) {
            missing.push(format!(
                "README.md says nothing about {provider} (no {token:?})"
            ));
        }
        if !features.contains(*token) {
            missing.push(format!(
                "docs/FEATURES.md says nothing about {provider} (no {token:?})"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "advertised but undocumented:\n  {}",
        missing.join("\n  ")
    );
}

/// Capabilities that are not top-level providers still ship, and
/// still have to be findable by a reader: rename's prepare step, the
/// semantic-token range request, and live reconfiguration.
#[test]
fn nested_capabilities_and_live_reconfiguration_are_documented() {
    let src = read("src/server.rs");
    let readme = read("README.md").to_lowercase();
    let features = read("docs/FEATURES.md").to_lowercase();
    let mut missing = Vec::new();

    let mut require = |advertised: bool, what: &str, tokens: &[&str]| {
        if !advertised {
            return;
        }
        for (page, text) in [("README.md", &readme), ("docs/FEATURES.md", &features)] {
            if !tokens.iter().any(|t| text.contains(t)) {
                missing.push(format!(
                    "{page} says nothing about {what} (none of {tokens:?})"
                ));
            }
        }
    };

    require(
        src.contains("prepare_provider: Some(true)"),
        "prepareRename",
        &["preparerename", "prepare rename"],
    );
    require(
        src.contains("range: Some(true)"),
        "semanticTokens/range",
        &["semantictokens/range", "`/range`"],
    );
    require(
        src.contains("async fn did_change_configuration"),
        "live reconfiguration",
        &[
            "didchangeconfiguration",
            "without a server restart",
            "apply live",
        ],
    );

    assert!(
        missing.is_empty(),
        "shipped but undocumented:\n  {}",
        missing.join("\n  ")
    );
}

/// Everything the server reads has to appear in the settings page:
/// initialization options, `OPENSIPS_LSP_*` environment fallbacks,
/// and the VS Code settings the client contributes.
#[test]
fn every_setting_the_server_reads_is_documented() {
    let server = read("src/server.rs");
    let features = read("docs/FEATURES.md");

    // initialization options: `opts.get("name")`
    let mut opts: BTreeSet<&str> = BTreeSet::new();
    for (_, rest) in server
        .match_indices("opts.get(\"")
        .map(|(i, _)| server.split_at(i))
    {
        if let Some(name) = rest
            .strip_prefix("opts.get(\"")
            .and_then(|r| r.split('"').next())
        {
            opts.insert(name);
        }
    }
    // the client namespace key is a wrapper, not a setting
    opts.remove("opensipsLsp");

    // environment fallbacks read anywhere in the crate (test-only
    // hooks are not user-facing settings)
    let mut envs: BTreeSet<String> = BTreeSet::new();
    for entry in std::fs::read_dir(format!("{}/src", root())).unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        for (i, _) in text.match_indices("OPENSIPS_LSP_") {
            let name: String = text[i..]
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_' || c.is_ascii_digit())
                .collect();
            if !name.contains("_TEST_") {
                envs.insert(name);
            }
        }
    }

    // VS Code settings contributed by the client
    let pkg: serde_json::Value = serde_json::from_str(&read("client/package.json")).unwrap();
    let vscode: BTreeSet<String> = pkg["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("client contributes configuration properties")
        .keys()
        .cloned()
        .collect();

    assert!(!opts.is_empty() && !envs.is_empty() && !vscode.is_empty());

    let mut missing = Vec::new();
    for o in &opts {
        if !features.contains(&format!("`{o}`")) {
            missing.push(format!("init option {o}"));
        }
    }
    for e in &envs {
        if !features.contains(e.as_str()) {
            missing.push(format!("environment variable {e}"));
        }
    }
    for v in &vscode {
        if !features.contains(&format!("`{v}`")) {
            missing.push(format!("VS Code setting {v}"));
        }
    }
    assert!(
        missing.is_empty(),
        "read by the server but absent from docs/FEATURES.md:\n  {}",
        missing.join("\n  ")
    );
}

/// A paragraph wedged between two rows ends the Markdown table.  The
/// rows below it become a second, header-less table — on GitHub and
/// on the Open VSX listing the first orphan is rendered as a header
/// row and the rest lose their column meaning.  A blank line before
/// the paragraph hides this from the eye, so the gate is: inside the
/// Settings section every table row belongs to ONE contiguous block.
#[test]
fn the_settings_table_renders_as_one_table() {
    let features = read("docs/FEATURES.md");
    let section: Vec<&str> = features
        .lines()
        .skip_while(|l| !l.starts_with("## Settings"))
        .take_while(|l| !l.starts_with("## Notes"))
        .collect();
    let header = section
        .iter()
        .position(|l| l.starts_with("| VS Code setting"))
        .expect("settings table header row");

    let contiguous = section[header..]
        .iter()
        .take_while(|l| l.starts_with('|'))
        .count();
    let total = section.iter().filter(|l| l.starts_with('|')).count();

    assert_eq!(
        contiguous,
        total,
        "the settings table is split: {} of its {total} rows sit below a non-table line, \
         so they render outside the table.  First stray line:\n  {:?}",
        total - contiguous,
        section[header + contiguous..]
            .iter()
            .find(|l| !l.trim().is_empty())
            .unwrap_or(&"")
    );
    assert!(total > 8, "settings table has only {total} rows");
}
