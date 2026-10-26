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

/// Every workflow file, by name and text.
fn all_workflows() -> Vec<(String, String)> {
    let dir = format!("{}/.github/workflows", env!("CARGO_MANIFEST_DIR"));
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect(".github/workflows")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    assert!(out.len() >= 3, "workflows not found under {dir}");
    out
}

/// A tag or a branch is a moving reference: whoever controls the
/// action's repository changes what runs here, with this repository's
/// token and secrets (`OVSX_PAT`). A full commit SHA does not move.
/// The comment names the release, so Dependabot can raise it and a
/// reader can see what it is.
#[test]
fn every_action_is_pinned_to_a_full_commit_sha() {
    let pinned = regex::Regex::new(r"^[^@\s]+@[0-9a-f]{40} # v[0-9][0-9.]*$").unwrap();
    let mut bad = Vec::new();
    let mut seen = 0usize;
    for (f, y) in all_workflows() {
        for (n, line) in y.lines().enumerate() {
            let t = line.trim_start().trim_start_matches("- ");
            let Some(r) = t.strip_prefix("uses:") else {
                continue;
            };
            let r = r.trim();
            if r.starts_with("./") {
                continue;
            }
            seen += 1;
            if !pinned.is_match(r) {
                bad.push(format!("{f}:{}: {r}", n + 1));
            }
        }
    }
    assert!(seen > 20, "too few `uses:` lines read: {seen}");
    assert!(
        bad.is_empty(),
        "not pinned to a commit SHA:\n{}",
        bad.join("\n")
    );
}

/// Every workflow starts from a read-only token, and a job that needs
/// more asks for it in its own `permissions:` block. Without a
/// top-level block a push event gets the repository default, which
/// can be write on everything.
#[test]
fn every_workflow_starts_from_a_read_only_token() {
    let mut bad = Vec::new();
    for (f, y) in all_workflows() {
        let head = y.split("\njobs:").next().unwrap_or("");
        let top: Vec<&str> = head
            .split("\npermissions:")
            .nth(1)
            .map(|p| {
                p.lines()
                    .take_while(|l| l.is_empty() || l.starts_with(' ') || l.starts_with('{'))
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .collect()
            })
            .unwrap_or_default();
        let read_only = head.contains("\npermissions: {}")
            || (!top.is_empty() && top.iter().all(|l| l.ends_with(": read")));
        if !read_only {
            bad.push(format!("{f}: top-level permissions {top:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// `actions/checkout` writes the job's token into `.git/config` unless
/// told not to, where every later step and action can read it. No job
/// here pushes with git credentials from a checkout.
#[test]
fn no_checkout_leaves_the_token_in_git_config() {
    let mut bad = Vec::new();
    for (f, y) in all_workflows() {
        let lines: Vec<&str> = y.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            if !line.contains("uses: actions/checkout@") {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            let step: Vec<&str> = lines[n + 1..]
                .iter()
                .take_while(|l| {
                    let i = l.len() - l.trim_start().len();
                    l.trim().is_empty() || i > indent
                })
                .copied()
                .collect();
            if !step
                .iter()
                .any(|l| l.trim() == "persist-credentials: false")
            {
                bad.push(format!("{f}:{}", n + 1));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "checkout without persist-credentials: false:\n{}",
        bad.join("\n")
    );
}

/// Code scanning analyses all three languages in the repository, and
/// the OpenSSF Scorecard reports the supply-chain checks to the same
/// Security tab. Before these, code scanning said "no analysis found".
#[test]
fn code_scanning_covers_rust_typescript_and_actions() {
    let codeql = workflow("codeql.yml").expect("codeql.yml must exist");
    for lang in ["rust", "javascript-typescript", "actions"] {
        assert!(
            codeql.contains(&format!("language: {lang}")),
            "CodeQL does not analyse {lang}"
        );
    }
    assert!(codeql.contains("github/codeql-action/analyze@"));
    assert!(codeql.contains("security-events: write"));
    let scorecard = workflow("scorecard.yml").expect("scorecard.yml must exist");
    assert!(scorecard.contains("ossf/scorecard-action@"));
    assert!(scorecard.contains("github/codeql-action/upload-sarif@"));
    assert!(scorecard.contains("publish_results: true"));
}
