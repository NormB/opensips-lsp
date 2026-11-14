//! Runner gate: every workflow job names a fixed runner image.
//!
//! A `-latest` label is moved by GitHub to the next OS version on
//! GitHub's schedule, and a retired image is refused outright. On
//! 2026-11-14 the v0.28.2 release run lost both macOS jobs before a
//! step ran, with "The configuration 'macos-14-arm64-us-default' is not
//! supported": the macOS 14 images had been retired on 2026-11-02, and
//! the label was read only when a tag was pushed. A tag never moves, so
//! that version could not be released.
//!
//! [`IMAGES`] is the whole list a workflow may name. Changing an image
//! is a change to this list, made on purpose and seen in review.

/// The runner images a workflow may name: fixed versions, each listed
/// in the table of <https://github.com/actions/runner-images>.
const IMAGES: [&str; 5] = [
    "ubuntu-24.04",
    "ubuntu-24.04-arm",
    "macos-26",
    "windows-2025",
    "windows-11-arm",
];

/// The one expression a `runs-on:` may hold: the matrix row's `runner:`,
/// which is itself checked.
const MATRIX_RUNNER: &str = "${{ matrix.runner }}";

/// Whether a workflow may name `label`: `Err` says why not.
fn label_verdict(label: &str) -> Result<(), String> {
    if label == MATRIX_RUNNER || IMAGES.contains(&label) {
        return Ok(());
    }
    if label.ends_with("-latest") {
        return Err(format!(
            "`{label}` floats: GitHub moves it to the next OS version; name one of {IMAGES:?}"
        ));
    }
    Err(format!(
        "`{label}` is not one of the fixed runner images {IMAGES:?}"
    ))
}

/// Every runner label `workflow` names, with its 1-based line: the
/// value of each `runs-on:` key and of each matrix `runner:` key.
fn runner_labels(workflow: &str) -> Vec<(usize, String)> {
    let mut labels = Vec::new();
    for (index, raw) in workflow.lines().enumerate() {
        let line = raw.trim().trim_start_matches("- ");
        let Some(value) = line
            .strip_prefix("runs-on:")
            .or_else(|| line.strip_prefix("runner:"))
        else {
            continue;
        };
        let value = value.split(" #").next().unwrap_or_default().trim();
        let value = value.trim_matches(|c| c == '"' || c == '\'');
        labels.push((index + 1, value.to_string()));
    }
    labels
}

/// Every workflow file's name and text.
fn workflows() -> Vec<(String, String)> {
    let dir = format!("{}/.github/workflows", env!("CARGO_MANIFEST_DIR"));
    let mut found: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect(".github/workflows")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "yml" || e == "yaml"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .to_string();
            (
                name,
                std::fs::read_to_string(&path).expect("a readable workflow"),
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_fixed_image_is_accepted() {
    for image in IMAGES {
        assert_eq!(label_verdict(image), Ok(()), "the fixed image {image}");
    }
}

#[test]
fn the_matrix_runner_expression_is_accepted() {
    assert_eq!(
        label_verdict("${{ matrix.runner }}"),
        Ok(()),
        "the matrix row is checked itself"
    );
}

#[test]
fn a_latest_label_is_refused_as_floating() {
    for label in ["ubuntu-latest", "windows-latest", "macos-latest"] {
        let told = label_verdict(label).expect_err("a floating label must be refused");
        assert!(told.contains("floats"), "the reason for {label}: {told}");
    }
}

#[test]
fn a_retired_image_is_refused() {
    let told = label_verdict("macos-14").expect_err("macos-14 was retired on 2026-11-02");
    assert!(
        told.contains("`macos-14` is not one of"),
        "the reason: {told}"
    );
}

#[test]
fn an_image_outside_the_list_is_refused() {
    for label in ["ubuntu-26.04", "macos-15", "self-hosted", ""] {
        assert!(
            label_verdict(label).is_err(),
            "`{label}` is not in the list"
        );
    }
}

#[test]
fn another_expression_is_refused() {
    assert!(
        label_verdict("${{ matrix.os }}").is_err(),
        "only matrix.runner is checked at its row"
    );
}

#[test]
fn a_label_is_matched_whole() {
    for label in ["ubuntu-24.04-latest", "macos-26-xlarge", "xubuntu-24.04"] {
        assert!(
            label_verdict(label).is_err(),
            "`{label}` only resembles a fixed image"
        );
    }
}

#[test]
fn labels_are_read_from_runs_on_and_from_matrix_rows() {
    let workflow = "jobs:\n  a:\n    runs-on: ubuntu-24.04\n    strategy:\n      matrix:\n        \
                    include:\n          - arch: x86_64\n            runner: macos-26\n          - \
                    runner: windows-2025\n    steps: []\n";
    assert_eq!(
        runner_labels(workflow),
        [
            (3, "ubuntu-24.04".to_string()),
            (8, "macos-26".to_string()),
            (9, "windows-2025".to_string())
        ],
        "one label per runs-on and per matrix runner"
    );
}

#[test]
fn a_trailing_comment_and_quotes_are_not_part_of_the_label() {
    let workflow = "    runs-on: \"ubuntu-24.04\" # the fixed image\n    runs-on: 'macos-26'\n";
    assert_eq!(
        runner_labels(workflow),
        [(1, "ubuntu-24.04".to_string()), (2, "macos-26".to_string())],
        "the label without its comment and quotes"
    );
}

#[test]
fn an_expression_is_read_whole() {
    assert_eq!(
        runner_labels("    runs-on: ${{ matrix.runner }}\n"),
        [(1, "${{ matrix.runner }}".to_string())],
        "the expression is the label"
    );
}

#[test]
fn other_keys_and_comments_name_no_label() {
    let workflow = "# runs-on: ubuntu-latest\n    name: runner: x\n    runner_name: y\n";
    assert_eq!(
        runner_labels(workflow),
        [],
        "no runs-on or runner key on these lines"
    );
}

/// The gate itself: every label of every workflow.
#[test]
fn every_workflow_names_fixed_runner_images() {
    let mut labels = 0;
    let mut refused = Vec::new();
    for (name, text) in workflows() {
        let found = runner_labels(&text);
        // POSITIVE CONTROL: a workflow with no label was not read right.
        assert!(!found.is_empty(), "{name} names no runner at all");
        for (line, label) in found {
            labels += 1;
            if let Err(told) = label_verdict(&label) {
                refused.push(format!(".github/workflows/{name}:{line}: {told}"));
            }
        }
    }
    assert!(
        labels >= 20,
        "expected the workflows' runner labels, found {labels}"
    );
    assert!(
        refused.is_empty(),
        "runner labels that are not fixed images:\n{}",
        refused.join("\n")
    );
}

/// The release builds both macOS servers on the one fixed macOS image:
/// the arm64 one natively and the x86_64 one by cross compiling, since
/// GitHub offers no Intel macOS runner on the standard plan.
#[test]
fn the_release_builds_both_macos_servers_on_the_fixed_image() {
    let (_, release) = workflows()
        .into_iter()
        .find(|(name, _)| name == "release.yml")
        .expect("release.yml must exist");
    let macos: Vec<String> = runner_labels(&release)
        .into_iter()
        .map(|(_, label)| label)
        .filter(|label| label.starts_with("macos"))
        .collect();
    assert_eq!(
        macos,
        ["macos-26", "macos-26"],
        "one macOS row per architecture"
    );
    assert_eq!(
        release.matches("target: x86_64-apple-darwin").count(),
        1,
        "the x86_64 macOS server is cross compiled on the arm64 runner"
    );
}

/// The lines of `workflow` that make up its macOS matrix rows: the
/// runner, the cross target and the oldest macOS the binary runs on.
fn macos_rows(workflow: &str) -> Vec<String> {
    workflow
        .lines()
        .map(|line| line.trim().trim_start_matches("- ").to_string())
        .filter(|line| {
            line == "runner: macos-26"
                || line.starts_with("minos:")
                || (line.starts_with("target: ") && line.ends_with("-apple-darwin"))
        })
        .collect()
}

/// CI builds the release's macOS rows on every pull request, so a
/// retired image or a raised minimum macOS version is found there and
/// not on a pushed tag. The rows are written twice; this holds them
/// together, and to the minimums v0.28.1's assets report.
#[test]
fn ci_builds_the_macos_rows_the_release_builds() {
    let all = workflows();
    let text = |wanted: &str| {
        all.iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| panic!("{wanted} must exist"))
    };
    let expected = [
        "runner: macos-26",
        "minos: \"11.0\"",
        "runner: macos-26",
        "minos: \"10.12\"",
        "target: x86_64-apple-darwin",
    ];
    assert_eq!(
        macos_rows(&text("release.yml")),
        expected,
        "release.yml's macOS rows"
    );
    assert_eq!(macos_rows(&text("ci.yml")), expected, "ci.yml's macOS rows");
    let ci = text("ci.yml");
    assert!(
        ci.contains("MACOSX_DEPLOYMENT_TARGET: ${{ matrix.minos }}")
            && text("release.yml").contains("MACOSX_DEPLOYMENT_TARGET=\"${{ matrix.minos }}\""),
        "both builds set the deployment target from the row"
    );
    let needs = ci
        .lines()
        .find(|line| line.trim_start().starts_with("needs: [") && line.contains("hooks"))
        .expect("the aggregate job's needs line");
    assert!(
        needs.contains("macos-servers"),
        "CI success must wait for the macOS builds: {needs}"
    );
}
