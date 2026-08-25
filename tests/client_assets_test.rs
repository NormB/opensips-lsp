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

/// An included fragment is rarely named so a filename pattern could
/// catch it, and claiming `*.cfg` outright would hijack every
/// unrelated config file in the workspace — the one thing the
/// association gate below deliberately refuses to allow.  What is
/// left is to ask the server, which has read the configuration that
/// says what includes what, and set the language at runtime.
///
/// Two things must hold for that to ever happen and neither is
/// visible to `tsc` or to `cargo`:
///
///   * the extension has to be RUNNING in a workspace whose only
///     OpenSIPS file on screen is an unassociated fragment, so
///     activation cannot rest on the language association alone;
///   * the method it asks with has to be the method the server
///     registers.  A rename on either side compiles cleanly in both
///     languages and fails only at runtime, in the one situation this
///     feature exists for.
#[test]
fn an_included_fragment_is_associated_at_runtime() {
    let root = env!("CARGO_MANIFEST_DIR");
    let v: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/client/package.json")).unwrap(),
    )
    .unwrap();
    let ext = std::fs::read_to_string(format!("{root}/client/src/extension.ts")).unwrap();
    let main = std::fs::read_to_string(format!("{root}/src/main.rs")).unwrap();

    // the name is taken from the server's own registration, so
    // retiring the request retires this requirement instead of
    // leaving a gate demanding a dead method
    let method = main
        .split("custom_method(")
        .nth(1)
        .and_then(|t| t.split('"').nth(1))
        .expect("the server registers a custom method");
    assert!(
        ext.contains(&format!("'{method}'")) || ext.contains(&format!("\"{method}\"")),
        "the extension never asks for {method}"
    );
    assert!(
        ext.contains("setTextDocumentLanguage"),
        "the extension asks but does nothing with the answer"
    );
    // a contributed switch nothing reads is a promise the page keeps
    // and the code breaks
    assert!(
        v["contributes"]["configuration"]["properties"]
            .get("opensipsLsp.associateIncludedFiles")
            .is_some(),
        "the association must be switchable"
    );
    assert!(
        ext.contains("associateIncludedFiles"),
        "the setting is contributed but nothing reads it"
    );

    let events: Vec<&str> = v["activationEvents"]
        .as_array()
        .expect("activationEvents")
        .iter()
        .filter_map(|e| e.as_str())
        .collect();
    assert!(
        events.iter().any(|e| e.starts_with("workspaceContains:")),
        "nothing starts the extension when the only file open is an \
         unassociated .cfg, so the runtime association can never run: {events:?}"
    );
    // every statically claimed pattern is also an activation trigger:
    // a root matching it is what makes its fragments findable
    for p in v["contributes"]["languages"][0]["filenamePatterns"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let p = p.as_str().unwrap();
        assert!(
            events.contains(&format!("workspaceContains:**/{p}").as_str()),
            "a workspace holding {p} must start the extension: {events:?}"
        );
    }
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
