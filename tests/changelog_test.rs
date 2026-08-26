//! Listing gate: the packaged CHANGELOG.md must exist and carry an
//! entry for the current version — marketplaces render it as the
//! "Changes" tab, and an empty tab is a trust gap.

#[test]
fn changelog_has_an_entry_for_the_current_version() {
    let root = env!("CARGO_MANIFEST_DIR");
    let version: String = {
        let manifest = std::fs::read_to_string(format!("{root}/client/package.json")).unwrap();
        serde_json::from_str::<serde_json::Value>(&manifest).unwrap()["version"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let changelog = std::fs::read_to_string(format!("{root}/client/CHANGELOG.md"))
        .expect("client/CHANGELOG.md must exist (rendered as the marketplace Changes tab)");
    assert!(
        changelog.contains(&format!("## [{version}]")),
        "CHANGELOG.md has no entry for the current version {version}"
    );
    // Server and client versions stay in lockstep. Compared against
    // `CARGO_PKG_VERSION` — the package version itself, and what the
    // server reports in `initialize` — rather than by searching
    // Cargo.toml for the text `version = "..."`, which also appears on
    // `rust-version` and on every dependency: a package version of
    // "1" or "1.94" satisfied that search without the two agreeing.
    let server = env!("CARGO_PKG_VERSION");
    assert_eq!(
        server, version,
        "server (Cargo.toml) {server} != extension (package.json) {version}"
    );
}

/// The newest entry must be the FIRST one.
///
/// The gate above asks only whether the changelog CONTAINS an entry
/// for the current version, so an entry appended to the bottom of the
/// file passes. Marketplaces render this file top-down as the
/// "Changes" tab: an entry that is present but buried under older
/// releases shows the reader the wrong release first.
#[test]
fn the_current_version_is_the_first_changelog_entry() {
    let root = env!("CARGO_MANIFEST_DIR");
    let manifest = std::fs::read_to_string(format!("{root}/client/package.json")).unwrap();
    let version = serde_json::from_str::<serde_json::Value>(&manifest).unwrap()["version"]
        .as_str()
        .unwrap()
        .to_string();
    let changelog = std::fs::read_to_string(format!("{root}/client/CHANGELOG.md")).unwrap();

    let entries: Vec<&str> = changelog
        .lines()
        .filter(|l| l.starts_with("## ["))
        .collect();
    // POSITIVE CONTROL: no entries would make the check below vacuous
    assert!(
        entries.len() >= 2,
        "expected a history of entries, found {}: {entries:?}",
        entries.len()
    );
    assert!(
        entries[0].starts_with(&format!("## [{version}]")),
        "the newest entry must be first; found {:?} above the entry for {version}",
        entries[0]
    );
}

/// The settings dropdown must offer exactly the releases the shipped
/// catalogue can answer for.
///
/// `opensipsVersion` is a dropdown in the extension's settings, and its
/// choices live in `client/package.json` while the releases live in
/// the vendored catalogue. Nothing connects the two files. Add a
/// release to the catalogue and the dropdown silently keeps offering
/// the old set; list one in the dropdown that the catalogue does not
/// carry and the user picks a release the server then reports as
/// unsupported, having been offered it by the settings UI.
#[test]
fn the_version_dropdown_offers_exactly_the_shipped_releases() {
    let root = env!("CARGO_MANIFEST_DIR");
    let manifest = std::fs::read_to_string(format!("{root}/client/package.json")).unwrap();
    let pkg: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let offered: Vec<String> =
        pkg["contributes"]["configuration"]["properties"]["opensipsLsp.opensipsVersion"]["enum"]
            .as_array()
            .expect("the setting must declare an enum, or it is a free-text box")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();

    let shipped: Vec<String> = opensips_lsp::catalog::builtin_versioned()
        .versions()
        .into_iter()
        .map(str::to_string)
        .collect();

    // POSITIVE CONTROL: an empty catalogue would make any dropdown
    // "match" the tail below.
    assert!(
        shipped.len() >= 2,
        "the catalogue claims {} release(s): {shipped:?}",
        shipped.len()
    );
    // the first choice is the empty default, meaning "the newest"
    assert_eq!(
        offered.first().map(String::as_str),
        Some(""),
        "the first choice must be the empty default: {offered:?}"
    );
    assert_eq!(
        &offered[1..],
        shipped.as_slice(),
        "the dropdown and the shipped catalogue disagree about which releases exist"
    );

    // every choice must carry a description, or the dropdown shows
    // bare version numbers with nothing explaining them
    let described = pkg["contributes"]["configuration"]["properties"]
        ["opensipsLsp.opensipsVersion"]["enumDescriptions"]
        .as_array()
        .expect("enumDescriptions")
        .len();
    assert_eq!(described, offered.len(), "one description per choice");
}
