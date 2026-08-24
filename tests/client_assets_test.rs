//! Gates on shipped client assets: the icon must be a real 256×256
//! RGBA PNG the manifest references, and security-sensitive settings
//! must be restricted in untrusted workspaces.

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

#[test]
fn icon_is_a_256_rgba_png_and_referenced() {
    let png = std::fs::read(root().join("client/icon.png")).expect("client/icon.png exists");
    assert!(png.len() > 33, "not a PNG");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(be32(&png[16..20]), 256, "width");
    assert_eq!(be32(&png[20..24]), 256, "height");
    assert_eq!(png[25], 6, "color type 6 = RGBA");

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("client/package.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["icon"], "icon.png");
}

#[test]
fn binary_path_settings_are_restricted_in_untrusted_workspaces() {
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root().join("client/package.json")).unwrap())
            .unwrap();
    let restricted = manifest["capabilities"]["untrustedWorkspaces"]["restrictedConfigurations"]
        .as_array()
        .expect("restrictedConfigurations");
    // both settings name a binary the extension will execute: a
    // workspace-committed settings.json must not control them in an
    // untrusted folder
    for key in ["opensipsLsp.opensipsPath", "opensipsLsp.serverPath"] {
        assert!(
            restricted.iter().any(|v| v == key),
            "{key} must be restricted, got {restricted:?}"
        );
    }
    assert_eq!(
        manifest["capabilities"]["untrustedWorkspaces"]["supported"],
        "limited"
    );
}

#[test]
fn file_association_does_not_claim_every_cfg_file() {
    // ".cfg" is a generic extension (wpa_supplicant.cfg, mpv.cfg,
    // build tool configs...) — claiming it globally steals unrelated
    // files.  The extension associates opensips.cfg by filename and a
    // documented glob; anything else is the user's files.associations
    // call.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/client/package.json"))
            .unwrap(),
    )
    .unwrap();
    let lang = &manifest["contributes"]["languages"][0];
    let exts = lang["extensions"].as_array().cloned().unwrap_or_default();
    assert!(
        !exts.iter().any(|e| e == ".cfg"),
        "must not claim the generic .cfg extension: {exts:?}"
    );
    let filenames = lang["filenames"].as_array().expect("filenames").clone();
    assert!(
        filenames.iter().any(|f| f == "opensips.cfg"),
        "opensips.cfg stays associated: {filenames:?}"
    );
    // split configs: *.opensips.cfg patterns keep working
    let patterns = lang["filenamePatterns"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        patterns.iter().any(|p| p == "*.opensips.cfg"),
        "a dedicated pattern for split configs: {patterns:?}"
    );

    // Every pattern the extension claims must be documented where a
    // user will look: widening the association silently is how you
    // end up hijacking someone's file with no explanation on the page
    // they read.
    // Both pages count.  The marketplace listing is what a prospective
    // user reads; the getting-started guide is where someone whose
    // file has no colours actually goes, and its "No colors" row said
    // a plain `.cfg` was enough long after the patterns had moved on.
    let pages = [
        (
            "client/README.md",
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/client/README.md"))
                .unwrap(),
        ),
        (
            "docs/GETTING_STARTED.md",
            std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/docs/GETTING_STARTED.md"
            ))
            .unwrap(),
        ),
    ];
    for p in &patterns {
        let p = p.as_str().unwrap();
        assert!(
            !p.starts_with("*.cfg") && p != "*",
            "{p} is as broad as claiming every .cfg"
        );
        for (name, text) in &pages {
            assert!(
                text.contains(p),
                "{name} does not tell the reader that {p} is claimed"
            );
        }
    }
    // A first-line marker, if used, must be one the real lexer
    // defines — inventing a marker would claim files on a rule the
    // parser has never heard of.
    if let Some(first) = lang["firstLine"].as_str() {
        assert!(
            first.starts_with("^#!"),
            "a first-line rule must anchor on the script-type marker: {first}"
        );
        for marker in ["KAMAILIO", "OPENSER", "SER", "MAXCOMPAT", "ALL"] {
            assert!(
                first.contains(marker),
                "{marker} is a script type src/core/cfg.lex accepts; the rule omits it"
            );
        }
        for (name, text) in &pages {
            assert!(
                text.contains("#!KAMAILIO"),
                "{name} must say a first-line marker is honoured"
            );
        }
    }
}
