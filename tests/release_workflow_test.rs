//! Drift gates for the GitHub workflows: fence the release traps that
//! past reruns proved real.  MS Marketplace publishing is deliberately
//! MANUAL (a vsce PAT would require an Azure DevOps account) — vsix
//! files are staged from the GitHub release instead.

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
    // missing secret skips (does not fail) the job
    assert!(
        section.contains(r#"if [ -z "$OVSX_PAT" ]"#),
        "missing OVSX_PAT must skip the job, not fail the release"
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
