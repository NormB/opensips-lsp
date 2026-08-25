//! Drift gates for the GitHub workflows: fence the release traps that
//! past reruns proved real.  Distribution targets are GitHub releases
//! and Open VSX only — the Microsoft VS Code Marketplace is not a
//! target and no publish logic for it may be added here.

fn workflow(name: &str) -> Option<String> {
    std::fs::read_to_string(format!(
        "{}/.github/workflows/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
    .ok()
}

#[test]
fn openvsx_publish_survives_reruns_and_set_e() {
    let y = workflow("release.yml").expect("release.yml must exist");
    // under `bash -e`, `out=$(cmd); rc=$?` dies before rc is read —
    // the capture must pre-seed rc and use `|| rc=$?`
    let (_, section) = y
        .split_once("OVSX_PAT")
        .expect("Open VSX publish job missing");
    assert!(
        section.contains("rc=0") && section.contains("|| rc=$?"),
        "Open VSX publish loop must capture rc set-e-safely"
    );
    // A missing secret must FAIL, not skip.  It used to skip: the
    // step printed a line and exited 0, so a release could cut the
    // tag, upload every GitHub asset and publish nothing at all, with
    // a green tick over the top.  Every installer would keep resolving
    // to the previous version and nothing anywhere would say why.
    assert!(
        section.contains(r#"if [ -z "$OVSX_PAT" ]"#),
        "the missing-secret branch went missing"
    );
    let missing = section
        .split_once(r#"if [ -z "$OVSX_PAT" ]"#)
        .map(|(_, rest)| rest.split("fi").next().unwrap_or(""))
        .unwrap_or("");
    assert!(
        missing.contains("exit 1") && !missing.contains("exit 0"),
        "a missing OVSX_PAT must fail the job, not skip it silently:\n{missing}"
    );

    // `ovsx` exits 0 having uploaded SOMETHING; the step has to check
    // it uploaded the version that was built, or a packaging slip
    // shipping the previous one passes as a release
    assert!(
        section.contains("$VERSION") && section.contains("without naming"),
        "the publish must assert the version it just uploaded"
    );

    // Uploaded is not installable.  The registry indexes
    // asynchronously — v0.20.0 was accepted immediately and served
    // roughly twenty minutes later — and until it is served, every
    // editor still offers the previous version.  A release is not
    // finished until the thing users install is the thing that was
    // released.
    assert!(
        section.contains("open-vsx.org/api/") && section.contains("still not served"),
        "the step must wait for the registry to actually serve the version"
    );
    // reruns of an already-accepted publish must not fail the job
    assert!(
        y.contains("already published"),
        "Open VSX rerun idempotency missing"
    );
}

#[test]
fn release_reruns_refresh_assets_instead_of_failing() {
    let y = workflow("release.yml").expect("release.yml must exist");
    assert!(
        y.contains("gh release view") && y.contains("--clobber"),
        "a rerun must upload --clobber onto the existing release"
    );
}

#[test]
fn workflows_use_action_versions_that_exist() {
    // actions/upload-artifact@v8 does not exist (v7 is current);
    // referencing it fails every job at "Set up job"
    for f in ["release.yml", "ci.yml", "wiki-sync.yml"] {
        let Some(y) = workflow(f) else { continue };
        assert!(
            !y.contains("upload-artifact@v8"),
            "{f}: actions/upload-artifact@v8 does not exist — use v7"
        );
    }
}

#[test]
fn no_ms_marketplace_publish_logic_exists() {
    // decision 2026-08-20: GitHub releases + Open VSX are the only
    // distribution targets — MS Marketplace logic must not come back
    for f in ["release.yml", "ci.yml", "wiki-sync.yml"] {
        let Some(y) = workflow(f) else { continue };
        for needle in ["VSCE_PAT", "vsce publish"] {
            assert!(
                !y.contains(needle),
                "{f}: '{needle}' found — MS Marketplace publishing is not a target"
            );
        }
    }
}
