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
