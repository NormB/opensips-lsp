//! Download gate: a workflow fetches a GitHub release asset only with
//! the job's token, and checks the asset's sha256.
//!
//! On 2026-11-25 the `fuzz readme` job of pull request #122 failed before
//! fuzzing: `curl` asked github.com for the cargo-fuzz 0.13.2 release
//! asset without a token and got HTTP 403 six times in 25 seconds, while
//! the three other fuzz jobs of the same run fetched the same file. A
//! request without a token is limited by the runner's shared address; a
//! request with the job's token is counted against that token. So a step
//! fetches a release asset with `gh release download`, sets `GH_TOKEN`
//! from `github.token`, and checks the file with `sha256sum -c`.

/// The path part of a release asset's URL on github.com.
const RELEASE_ASSET_PATH: &str = "/releases/download/";

/// The command that fetches a release asset with a token.
const GH_RELEASE_DOWNLOAD: &str = "gh release download";

/// The token lines a step that runs [`GH_RELEASE_DOWNLOAD`] may set.
const TOKEN_LINES: [&str; 2] = [
    "GH_TOKEN: ${{ github.token }}",
    "GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}",
];

/// The command that checks a downloaded file against its pinned sum.
const SHA256_CHECK: &str = "sha256sum -c";

/// One step of a workflow: its 1-based first line and its lines.
#[derive(Debug)]
struct Step {
    line: usize,
    lines: Vec<String>,
}

/// Whether `line` starts a step: a list item at the indentation of a
/// job's `steps:` list, that is, `- name:`, `- uses:` or `- run:`.
fn starts_step(line: &str) -> bool {
    let trimmed = line.trim_start();
    ["- name:", "- uses:", "- run:", "- id:", "- if:"]
        .iter()
        .any(|key| trimmed.starts_with(key))
}

/// Every step of `workflow`, in order. A line before the first step
/// belongs to no step.
fn steps(workflow: &str) -> Vec<Step> {
    let mut found: Vec<Step> = Vec::new();
    for (index, raw) in workflow.lines().enumerate() {
        if starts_step(raw) {
            found.push(Step {
                line: index + 1,
                lines: Vec::new(),
            });
        }
        if let Some(step) = found.last_mut() {
            step.lines.push(raw.to_string());
        }
    }
    found
}

/// Whether `line` is a comment: comments may name a URL freely.
fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

/// Whether `line` fetches a github.com release asset with a plain HTTP
/// client, which sends no token.
fn fetches_without_token(line: &str) -> bool {
    if is_comment(line) || !line.contains(RELEASE_ASSET_PATH) {
        return false;
    }
    line.contains("https://github.com/") || line.contains("http://github.com/")
}

/// Why `step` breaks the download rule, one reason per broken rule.
fn step_verdict(step: &Step) -> Vec<String> {
    let mut reasons = Vec::new();
    for (offset, line) in step.lines.iter().enumerate() {
        if fetches_without_token(line) {
            reasons.push(format!(
                "line {}: fetches a release asset without a token; use `{GH_RELEASE_DOWNLOAD}`",
                step.line + offset
            ));
        }
    }
    let code: Vec<&String> = step.lines.iter().filter(|l| !is_comment(l)).collect();
    if !code.iter().any(|l| l.contains(GH_RELEASE_DOWNLOAD)) {
        return reasons;
    }
    let has_token = code
        .iter()
        .any(|l| TOKEN_LINES.iter().any(|token| l.trim() == *token));
    if !has_token {
        reasons.push(format!(
            "line {}: the step runs `{GH_RELEASE_DOWNLOAD}` without one of {TOKEN_LINES:?}",
            step.line
        ));
    }
    if !code.iter().any(|l| l.contains(SHA256_CHECK)) {
        reasons.push(format!(
            "line {}: the step runs `{GH_RELEASE_DOWNLOAD}` without `{SHA256_CHECK}`",
            step.line
        ));
    }
    reasons
}

/// Every reason `workflow` breaks the download rule.
fn workflow_verdict(workflow: &str) -> Vec<String> {
    let mut reasons: Vec<String> = steps(workflow).iter().flat_map(step_verdict).collect();
    // a URL outside every step (a top-level `env:`) is fetched by some
    // step all the same
    let first_step = steps(workflow).first().map_or(usize::MAX, |s| s.line);
    for (index, line) in workflow.lines().enumerate() {
        if index + 1 < first_step && fetches_without_token(line) {
            reasons.push(format!(
                "line {}: names a release asset outside a step; fetch it with `{GH_RELEASE_DOWNLOAD}`",
                index + 1
            ));
        }
    }
    reasons
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

/// A step that fetches `asset` the way the rule asks.
fn good_step(asset: &str) -> String {
    format!(
        "      - name: install\n        env:\n          GH_TOKEN: ${{{{ github.token }}}}\n        run: |\n          gh release download 1.0 -R owner/tool -p {asset}\n          echo \"abc  {asset}\" | sha256sum -c -\n"
    )
}

// --- success: what the rule accepts ---

#[test]
fn a_step_with_the_token_and_the_check_is_accepted() {
    let workflow = format!("jobs:\n  a:\n    steps:\n{}", good_step("t.tar.gz"));
    assert_eq!(
        workflow_verdict(&workflow),
        Vec::<String>::new(),
        "{workflow}"
    );
}

#[test]
fn the_secrets_token_is_accepted() {
    let workflow = good_step("t.tar.gz").replace("github.token", "secrets.GITHUB_TOKEN");
    assert_eq!(
        workflow_verdict(&workflow),
        Vec::<String>::new(),
        "{workflow}"
    );
}

#[test]
fn a_step_that_downloads_nothing_is_accepted() {
    let workflow = "      - name: build\n        run: cargo build\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "a plain step"
    );
}

#[test]
fn curl_to_another_host_is_accepted() {
    let workflow = "      - name: probe\n        run: curl -s https://open-vsx.org/api/a/b/1.0\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "open-vsx.org"
    );
}

#[test]
fn curl_to_github_outside_releases_is_accepted() {
    let workflow = "      - name: probe\n        run: curl -s https://github.com/owner/repo/archive/v1.tar.gz\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "an archive URL"
    );
}

#[test]
fn a_release_url_in_a_comment_is_accepted() {
    let workflow = "      # from https://github.com/o/r/releases/download/1/a.tgz\n      - name: x\n        run: true\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "a comment"
    );
}

#[test]
fn gh_release_upload_needs_no_check() {
    let workflow = "      - name: publish\n        run: gh release upload v1 a.vsix\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "upload, not download"
    );
}

#[test]
fn gh_release_create_needs_no_check() {
    let workflow = "      - name: publish\n        run: gh release create v1 a.vsix\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "create, not download"
    );
}

#[test]
fn two_good_steps_are_accepted() {
    let workflow = format!("{}{}", good_step("a.tgz"), good_step("b.tgz"));
    assert_eq!(
        workflow_verdict(&workflow),
        Vec::<String>::new(),
        "{workflow}"
    );
}

#[test]
fn a_uses_step_is_accepted() {
    let workflow = "      - uses: actions/checkout@abc # v7\n        with:\n          persist-credentials: false\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "a uses step"
    );
}

#[test]
fn the_token_may_sit_after_the_run_block() {
    let workflow = "      - name: install\n        run: |\n          gh release download 1 -R o/t -p a.tgz\n          echo \"x  a.tgz\" | sha256sum -c -\n        env:\n          GH_TOKEN: ${{ github.token }}\n";
    assert_eq!(
        workflow_verdict(workflow),
        Vec::<String>::new(),
        "env after run"
    );
}

#[test]
fn every_workflow_fetches_release_assets_with_a_token_and_a_check() {
    let mut broken = Vec::new();
    for (name, text) in workflows() {
        for reason in workflow_verdict(&text) {
            broken.push(format!("{name} {reason}"));
        }
    }
    assert!(
        broken.is_empty(),
        "release downloads break the rule:\n{}",
        broken.join("\n")
    );
}

// --- failure: what the rule refuses ---

#[test]
fn curl_of_a_release_asset_is_refused() {
    let workflow = "      - name: x\n        run: curl -fsSLO https://github.com/o/r/releases/download/v1/a.tgz\n";
    let reasons = workflow_verdict(workflow);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].contains("line 2"), "the URL's line: {reasons:?}");
}

#[test]
fn wget_of_a_release_asset_is_refused() {
    let workflow =
        "      - name: x\n        run: wget https://github.com/o/r/releases/download/v1/a.tgz\n";
    assert_eq!(
        workflow_verdict(workflow).len(),
        1,
        "wget sends no token either"
    );
}

#[test]
fn a_url_on_a_continuation_line_is_refused() {
    let workflow = "      - name: x\n        run: |\n          curl -fsSL -o a.tgz \\\n            \"https://github.com/o/r/releases/download/${V}/a.tgz\"\n";
    let reasons = workflow_verdict(workflow);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(
        reasons[0].contains("line 4"),
        "the continuation line: {reasons:?}"
    );
}

#[test]
fn a_download_without_the_token_is_refused() {
    let workflow = good_step("a.tgz").replace("          GH_TOKEN: ${{ github.token }}\n", "");
    let reasons = workflow_verdict(&workflow);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].contains("without one of"), "{reasons:?}");
}

#[test]
fn a_download_without_the_check_is_refused() {
    let workflow = good_step("a.tgz").replace(" | sha256sum -c -", "");
    let reasons = workflow_verdict(&workflow);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].contains(SHA256_CHECK), "{reasons:?}");
}

#[test]
fn a_download_without_either_gives_both_reasons() {
    let workflow = "      - name: x\n        run: gh release download 1 -R o/t -p a.tgz\n";
    assert_eq!(workflow_verdict(workflow).len(), 2, "token and check");
}

#[test]
fn another_token_is_refused() {
    let workflow = good_step("a.tgz").replace("github.token", "secrets.MY_PAT");
    assert_eq!(workflow_verdict(&workflow).len(), 1, "only the job's token");
}

#[test]
fn a_token_in_the_next_step_does_not_count() {
    let workflow = "      - name: x\n        run: |\n          gh release download 1 -R o/t -p a.tgz\n          echo \"x  a.tgz\" | sha256sum -c -\n      - name: y\n        env:\n          GH_TOKEN: ${{ github.token }}\n        run: true\n";
    assert_eq!(
        workflow_verdict(workflow).len(),
        1,
        "the token belongs to step y"
    );
}

#[test]
fn a_check_in_the_next_step_does_not_count() {
    let workflow = "      - name: x\n        env:\n          GH_TOKEN: ${{ github.token }}\n        run: gh release download 1 -R o/t -p a.tgz\n      - name: y\n        run: echo \"x  a.tgz\" | sha256sum -c -\n";
    assert_eq!(
        workflow_verdict(workflow).len(),
        1,
        "the check belongs to step y"
    );
}

#[test]
fn a_commented_token_does_not_count() {
    let workflow = good_step("a.tgz").replace("          GH_TOKEN", "          # GH_TOKEN");
    assert_eq!(
        workflow_verdict(&workflow).len(),
        1,
        "a comment sets nothing"
    );
}

#[test]
fn a_commented_check_does_not_count() {
    let workflow = good_step("a.tgz").replace("          echo", "          # echo");
    assert_eq!(
        workflow_verdict(&workflow).len(),
        1,
        "a comment checks nothing"
    );
}

#[test]
fn a_release_url_in_top_level_env_is_refused() {
    let workflow = "env:\n  URL: https://github.com/o/r/releases/download/1/a.tgz\njobs:\n  a:\n    steps:\n      - name: x\n        run: curl -fsSLO \"$URL\"\n";
    let reasons = workflow_verdict(workflow);
    assert_eq!(reasons.len(), 1, "{reasons:?}");
    assert!(reasons[0].contains("line 2"), "{reasons:?}");
}

// --- edge: the reader's corners ---

#[test]
fn an_empty_workflow_has_no_steps() {
    assert!(steps("").is_empty(), "no lines, no steps");
    assert_eq!(
        workflow_verdict(""),
        Vec::<String>::new(),
        "nothing to refuse"
    );
}

#[test]
fn lines_before_the_first_step_belong_to_no_step() {
    let found = steps("name: CI\non: push\n      - name: x\n        run: true\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].line, 3, "{found:?}");
    assert_eq!(found[0].lines.len(), 2, "{found:?}");
}

#[test]
fn each_step_key_starts_a_step() {
    for key in ["- name: a", "- uses: a", "- run: a", "- id: a", "- if: a"] {
        assert!(starts_step(&format!("      {key}")), "{key}");
    }
}

#[test]
fn a_matrix_row_or_a_with_key_starts_no_step() {
    for line in [
        "        target: [a]",
        "          with:",
        "        - a",
        "on:",
    ] {
        assert!(!starts_step(line), "{line}");
    }
}

#[test]
fn steps_split_at_each_step_start() {
    let found = steps(&format!("{}{}", good_step("a"), good_step("b")));
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[1].line, 7, "{found:?}");
}

#[test]
fn http_github_is_refused_too() {
    assert!(
        fetches_without_token("curl http://github.com/o/r/releases/download/1/a"),
        "plain http sends no token either"
    );
}

#[test]
fn a_release_path_on_another_host_is_accepted() {
    assert!(
        !fetches_without_token("curl https://example.com/o/r/releases/download/1/a"),
        "only github.com is counted against the runner's address"
    );
}

#[test]
fn a_gh_download_line_is_not_a_tokenless_fetch() {
    assert!(
        !fetches_without_token("gh release download 1 -R o/t -p a.tgz"),
        "gh sends the token"
    );
}

#[test]
fn an_indented_comment_is_a_comment() {
    assert!(is_comment("          # note"), "indented");
    assert!(
        !is_comment("          run: x # note"),
        "a trailing comment is code"
    );
}

#[test]
fn a_token_line_must_match_whole() {
    let workflow = good_step("a.tgz").replace(
        "GH_TOKEN: ${{ github.token }}",
        "GH_TOKEN: ${{ github.token }}-x",
    );
    assert_eq!(
        workflow_verdict(&workflow).len(),
        1,
        "a changed token is another token"
    );
}

#[test]
fn reasons_name_the_step_line() {
    let workflow = "      - name: a\n        run: true\n      - name: x\n        run: gh release download 1 -R o/t -p a.tgz\n";
    let reasons = workflow_verdict(workflow);
    assert!(reasons.iter().all(|r| r.contains("line 3")), "{reasons:?}");
}

#[test]
fn the_workflows_directory_is_read() {
    let names: Vec<String> = workflows().into_iter().map(|(name, _)| name).collect();
    assert!(names.contains(&"ci.yml".to_string()), "{names:?}");
}
