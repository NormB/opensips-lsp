//! `docs/EDITORS.md` is the page for everyone not using VS Code, and
//! it is the page nothing was checking.
//!
//! It had drifted three ways at once: it told every non-VS-Code client
//! to match only `opensips.cfg` after the association widened, told
//! Helix to claim EVERY `.cfg` file — the one thing the extension
//! deliberately refuses to do — and presented `opensipsSrc` as
//! required after the catalogues became built in.  Each check below
//! derives its expectation from the source, so the page cannot drift
//! from the thing it documents again.

use std::collections::BTreeSet;

fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{}/{rel}", root())).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every filename pattern the extension claims must be shown to
/// readers configuring some other client, or they will configure a
/// narrower set and wonder why half their configs are inert.
#[test]
fn every_claimed_filename_pattern_is_shown_to_other_clients() {
    let manifest: serde_json::Value = serde_json::from_str(&read("client/package.json")).unwrap();
    let lang = &manifest["contributes"]["languages"][0];
    let doc = read("docs/EDITORS.md");

    let mut claimed: Vec<String> = lang["filenames"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|f| f.as_str().map(str::to_string))
        .collect();
    claimed.extend(
        lang["filenamePatterns"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|f| f.as_str().map(str::to_string)),
    );
    assert!(claimed.len() >= 2, "association shrank: {claimed:?}");

    for c in &claimed {
        assert!(
            doc.contains(c.as_str()),
            "docs/EDITORS.md never shows {c}, so a non-VS-Code user cannot match it"
        );
    }
}

/// The examples must not tell anyone to claim every `.cfg` on disk.
/// The extension refuses to, on purpose, and a Helix `file-types`
/// entry of a bare `"cfg"` does exactly that.
#[test]
fn no_example_claims_every_cfg_file() {
    let doc = read("docs/EDITORS.md");
    for (n, line) in doc.lines().enumerate() {
        let t = line.trim();
        // a bare "cfg" extension entry, or a glob of everything.
        // `opensips*.cfg` is NOT that: the quote has to sit directly
        // against the star for the pattern to mean every .cfg file.
        assert!(
            !(t.contains("\"cfg\"") || t.contains("\"*.cfg\"") || t.contains("'*.cfg'")),
            "docs/EDITORS.md:{} claims every .cfg file: {t}",
            n + 1
        );
    }
}

/// Environment variables named in the page must be ones the crate
/// actually reads.
#[test]
fn every_environment_variable_shown_is_one_the_server_reads() {
    let doc = read("docs/EDITORS.md");
    let mut real: BTreeSet<String> = BTreeSet::new();
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
            real.insert(name);
        }
    }
    assert!(!real.is_empty());

    let mut shown = BTreeSet::new();
    for (i, _) in doc.match_indices("OPENSIPS_LSP_") {
        let name: String = doc[i..]
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || *c == '_' || c.is_ascii_digit())
            .collect();
        shown.insert(name);
    }
    assert!(!shown.is_empty(), "the page shows no environment fallback");

    let invented: Vec<&String> = shown.difference(&real).collect();
    assert!(
        invented.is_empty(),
        "docs/EDITORS.md names environment variables the server never reads: {invented:?}"
    );
}

/// VS Code settings named on the page must be ones the client
/// contributes.
#[test]
fn every_vs_code_setting_shown_is_contributed() {
    let doc = read("docs/EDITORS.md");
    let manifest: serde_json::Value = serde_json::from_str(&read("client/package.json")).unwrap();
    let contributed: BTreeSet<String> = manifest["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("contributed properties")
        .keys()
        .cloned()
        .collect();

    let mut shown = BTreeSet::new();
    for (i, _) in doc.match_indices("opensipsLsp.") {
        let name: String = doc[i..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '.')
            .collect();
        // prose names the prefix itself ("settings live under
        // `opensipsLsp.`"); only a prefix WITH a key is a setting
        if name.ends_with('.') {
            continue;
        }
        shown.insert(name);
    }
    assert!(!shown.is_empty(), "the page names no VS Code settings");

    let invented: Vec<&String> = shown.difference(&contributed).collect();
    assert!(
        invented.is_empty(),
        "docs/EDITORS.md names settings the client does not contribute: {invented:?}"
    );
}

/// The CLI section is the answer to "can I use this without an
/// editor", so its usage line must be the real one.
#[test]
fn the_documented_cli_usage_is_the_real_usage() {
    let cli = read("src/cli.rs");
    let start = cli.find("usage: opensips-lsp check").expect("usage string");
    let usage: String = cli[start..]
        .chars()
        .take_while(|c| *c != '"')
        .collect::<String>()
        .trim_end()
        .to_string();
    let doc = read("docs/EDITORS.md");
    let shown = usage.trim_start_matches("usage: ");
    assert!(
        doc.contains(shown),
        "docs/EDITORS.md does not show the real usage line: {shown:?}"
    );
}
