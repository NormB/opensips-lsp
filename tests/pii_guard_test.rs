//! Repo hygiene gate: no personal infrastructure may appear anywhere
//! in the tracked tree — LAN/WAN addresses, private hostnames or
//! domains, or real home-directory paths. Runs in CI, so a leak is a
//! red build, not a post-hoc scrub.

use std::path::Path;

/// Patterns that must never appear in the repository. Documentation
/// address space (192.0.2.0/24, example.com) and generic placeholders
/// (`/home/user`, `/path/to`) are the allowed alternatives.
const BANNED: &[(&str, &str)] = &[
    (r"10\.0\.0\.[0-9]", "RFC1918 lab address"),
    (r"192\.168\.[0-9]", "RFC1918 address"),
    (r"172\.(1[6-9]|2[0-9]|3[01])\.[0-9]", "RFC1918 address"),
    (r"63\.133\.[0-9]", "WAN address"),
    (r"goes\.com", "private domain"),
    (r"\bcarbon\b", "private hostname"),
    (r"\bvoxcortex\b", "private cluster name"),
    (r"\bminer1\b", "private hostname"),
    (r"opensips-1", "private hostname"),
    (r"/home/(?!user\b)[a-z][a-z0-9]*", "real home path"),
    (r"n\.brandinger@", "personal email in content"),
];

// the OnceLock compiles the inner regex exactly once; clippy cannot
// see through get_or_init.
#[allow(clippy::regex_creation_in_loops)]
fn find_pii(text: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for (pat, why) in BANNED {
        // fancy_regex for the lookahead in the home-path pattern is
        // overkill; emulate it: match /home/<name> and allow "user".
        if pat.starts_with("/home/") {
            static HOME_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            let re = HOME_RE.get_or_init(|| regex::Regex::new(r"/home/([a-z][a-z0-9]*)").unwrap());
            for c in re.captures_iter(text) {
                if &c[1] != "user" {
                    hits.push(format!("{} ({})", &c[0], why));
                }
            }
        } else {
            let re = regex::Regex::new(pat).unwrap();
            if let Some(m) = re.find(text) {
                hits.push(format!("{} ({})", m.as_str(), why));
            }
        }
    }
    hits
}

#[test]
fn detector_fires_on_planted_pii() {
    assert!(!find_pii("nats://10.0.0.31:4222").is_empty());
    assert!(!find_pii("ssh gator@63.133.236.8").is_empty());
    assert!(!find_pii("git@gitlab.goes.com:x/y.git").is_empty());
    assert!(!find_pii("/home/gator/scratch/tree").is_empty());
    assert!(!find_pii("runs on opensips-1 and miner1").is_empty());
}

#[test]
fn detector_allows_documentation_placeholders() {
    assert!(find_pii("nats://192.0.2.10:4222").is_empty());
    assert!(find_pii("/home/user/src/opensips").is_empty());
    assert!(find_pii("/path/to/opensips").is_empty());
    assert!(find_pii("sip:probe@example.com").is_empty());
    assert!(find_pii("Copyright (c) 2026 Norm Brandinger").is_empty());
}

fn walk(dir: &Path, hits: &mut Vec<String>) {
    let skip = ["target", ".git", "graphify-out", "node_modules", "out"];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_dir() {
            if !skip.contains(&name.as_str()) {
                walk(&p, hits);
            }
        } else if let Ok(text) = std::fs::read_to_string(&p) {
            for h in find_pii(&text) {
                hits.push(format!("{}: {}", p.display(), h));
            }
        }
    }
}

#[test]
fn repository_tree_is_free_of_personal_infrastructure() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    walk(root, &mut hits);
    // this file plants examples on purpose; everything else must be clean
    hits.retain(|h| !h.contains("pii_guard_test.rs"));
    assert!(
        hits.is_empty(),
        "personal infrastructure leaked:\n{}",
        hits.join("\n")
    );
}
