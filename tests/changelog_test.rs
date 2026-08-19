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
    // server and client versions stay in lockstep
    let cargo = std::fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(
        cargo.contains(&format!("version = \"{version}\"")),
        "client {version} != server version"
    );
}
