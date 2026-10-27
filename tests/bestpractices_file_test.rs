//! `.bestpractices.json` in the repository root proposes answers to the
//! OpenSSF Best Practices "passing" criteria; the site reads it when the
//! project is analysed.  The site skips the whole file when it is not a
//! JSON object or is over 100,000 bytes, skips a status it does not
//! recognise and a key it does not know, and says nothing about any of
//! it.  These tests keep the file readable, hold it to the full set of
//! criteria, and keep every repository path it links to in the tree.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::{Map, Value};

/// The largest file the site reads.
const MAX_FILE: usize = 100_000;
/// The longest justification the site stores.
const MAX_TEXT: usize = 8192;
/// Links into this repository's main branch.
const MAIN: &str = "https://github.com/NormB/opensips-lsp/";

/// Every "passing" criterion except `know_secure_design` and
/// `know_common_errors`, which describe the maintainer rather than the
/// repository and are answered by the maintainer on the site.
const PASSING: [&str; 65] = [
    "description_good",
    "interact",
    "contribution",
    "contribution_requirements",
    "floss_license",
    "floss_license_osi",
    "license_location",
    "documentation_basics",
    "documentation_interface",
    "sites_https",
    "discussion",
    "english",
    "maintained",
    "repo_public",
    "repo_track",
    "repo_interim",
    "repo_distributed",
    "version_unique",
    "version_semver",
    "version_tags",
    "release_notes",
    "release_notes_vulns",
    "report_process",
    "report_tracker",
    "report_responses",
    "enhancement_responses",
    "report_archive",
    "vulnerability_report_process",
    "vulnerability_report_private",
    "vulnerability_report_response",
    "build",
    "build_common_tools",
    "build_floss_tools",
    "test",
    "test_invocation",
    "test_most",
    "test_continuous_integration",
    "test_policy",
    "tests_are_added",
    "tests_documented_added",
    "warnings",
    "warnings_fixed",
    "warnings_strict",
    "crypto_published",
    "crypto_call",
    "crypto_floss",
    "crypto_keylength",
    "crypto_working",
    "crypto_weaknesses",
    "crypto_pfs",
    "crypto_password_storage",
    "crypto_random",
    "delivery_mitm",
    "delivery_unsigned",
    "vulnerabilities_fixed_60_days",
    "vulnerabilities_critical_fixed",
    "no_leaked_credentials",
    "static_analysis",
    "static_analysis_common_vulnerabilities",
    "static_analysis_fixed",
    "static_analysis_often",
    "dynamic_analysis",
    "dynamic_analysis_unsafe",
    "dynamic_analysis_enable_assertions",
    "dynamic_analysis_fixed",
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn text() -> String {
    let path = root().join(".bestpractices.json");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert!(text.len() <= MAX_FILE, "{} bytes", text.len());
    text
}

fn answers() -> Map<String, Value> {
    match serde_json::from_str(&text()).expect("valid JSON") {
        Value::Object(map) => map,
        other => panic!("not a JSON object: {other}"),
    }
}

#[test]
fn every_status_is_one_the_site_accepts() {
    let map = answers();
    let statuses: Vec<_> = map.iter().filter(|(k, _)| k.ends_with("_status")).collect();
    assert!(!statuses.is_empty(), "no answers");
    for (key, value) in statuses {
        let status = value
            .as_str()
            .unwrap_or_else(|| panic!("{key} is not text"));
        assert!(
            ["Met", "Unmet", "N/A"].contains(&status),
            "{key} is {status:?}"
        );
    }
}

#[test]
fn every_status_has_its_justification_and_no_key_is_stray() {
    let map = answers();
    for (key, value) in &map {
        if let Some(criterion) = key.strip_suffix("_status") {
            let justification = map
                .get(&format!("{criterion}_justification"))
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("{criterion} has no justification"));
            assert!(!justification.trim().is_empty(), "{criterion}: empty");
            assert!(justification.len() <= MAX_TEXT, "{criterion}: too long");
        } else if let Some(criterion) = key.strip_suffix("_justification") {
            assert!(value.is_string(), "{key} is not text");
            assert!(
                map.contains_key(&format!("{criterion}_status")),
                "{key} has no status"
            );
        } else {
            panic!("{key} is neither a status nor a justification");
        }
    }
}

#[test]
fn every_passing_criterion_is_answered_and_no_other() {
    let map = answers();
    let answered: BTreeSet<&str> = map
        .keys()
        .filter_map(|k| k.strip_suffix("_status"))
        .collect();
    let expected: BTreeSet<&str> = PASSING.into_iter().collect();
    let missing: Vec<_> = expected.difference(&answered).collect();
    let unknown: Vec<_> = answered.difference(&expected).collect();
    assert!(
        missing.is_empty() && unknown.is_empty(),
        "missing {missing:?}, not a passing criterion {unknown:?}"
    );
}

/// The repository paths, in order of appearance, that `text` links to on
/// main, without a trailing `#fragment`, `?query` or prose punctuation.
fn linked_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for kind in ["blob/main/", "tree/main/"] {
        let prefix = format!("{MAIN}{kind}");
        let mut rest = text;
        while let Some(at) = rest.find(&prefix) {
            rest = &rest[at + prefix.len()..];
            let end = rest
                .find(|c: char| c.is_whitespace() || "\"'<>()#?".contains(c))
                .unwrap_or(rest.len());
            paths.push(
                rest[..end]
                    .trim_end_matches(['.', ',', ':', ';', '!'])
                    .to_string(),
            );
        }
    }
    paths
}

/// The paths `git ls-files` lists, and every directory above them.
fn tracked() -> BTreeSet<String> {
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    let mut set = BTreeSet::new();
    for file in String::from_utf8_lossy(&out.stdout).split('\0') {
        let mut path = file;
        while !path.is_empty() {
            set.insert(path.to_string());
            path = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        }
    }
    set
}

#[test]
fn linked_paths_reads_blob_and_tree_links() {
    let text = format!(
        "see {MAIN}blob/main/SECURITY.md. and {MAIN}tree/main/LICENSES, \
         {MAIN}blob/main/CONTRIBUTING.md#reporting-bugs and {MAIN}issues"
    );
    assert_eq!(
        linked_paths(&text),
        ["SECURITY.md", "CONTRIBUTING.md", "LICENSES"]
    );
}

#[test]
fn every_path_it_links_to_is_tracked() {
    let text = text();
    let paths = linked_paths(&text);
    assert!(!paths.is_empty(), "no repository links");
    let tracked = tracked();
    let gone: Vec<_> = paths.iter().filter(|p| !tracked.contains(*p)).collect();
    assert!(gone.is_empty(), "links to paths not in the tree: {gone:?}");
}
