//! Every release carries a signature and a provenance file AS ASSETS.
//!
//! OpenSSF Scorecard's Signed-Releases check scored 0 on every release
//! up to v0.28.0: no asset was a signature or a provenance file.
//! Scorecard reads only the asset names of the last five releases, and
//! counts a signature by suffix (`probes/releasesAreSigned/impl.go`) and
//! provenance by suffix (`probes/releasesHaveProvenance/impl.go`).
//!
//! So the `publish` job must, from its own OIDC identity and with no
//! stored key:
//!
//! - write `SHA256SUMS.txt` over every asset;
//! - attest build provenance with `actions/attest-build-provenance`,
//!   and copy the bundle it writes to an `.intoto.jsonl` file;
//! - sign `SHA256SUMS.txt` with `cosign sign-blob` into a
//!   `.sigstore.json` bundle;
//! - verify both files the way SECURITY.md tells a user to, before the
//!   release exists;
//! - upload both, in the `gh release create` call and in the
//!   `gh release upload` call a rerun takes.
//!
//! The checker is a pure function of the workflow text, split into jobs
//! and steps by indentation. The fixtures at the bottom drive it with
//! each fault it exists to catch.

/// Scorecard's signature suffixes, from
/// `probes/releasesAreSigned/impl.go` (ossf/scorecard at 6b98ea0a6a,
/// read 2026-10-28).
const SCORECARD_SIGNATURE_SUFFIXES: &[&str] = &[
    ".asc",
    ".minisig",
    ".sig",
    ".sign",
    ".sigstore",
    ".sigstore.json",
];

/// Scorecard's provenance suffixes, from
/// `probes/releasesHaveProvenance/impl.go` at the same commit.
const SCORECARD_PROVENANCE_SUFFIXES: &[&str] = &[".intoto.jsonl"];

/// The OIDC issuer of every GitHub Actions token.
const ACTIONS_ISSUER: &str = "https://token.actions.githubusercontent.com";

/// The file the release signs: it names every other asset.
const SIGNED_BLOB: &str = "SHA256SUMS.txt";

fn workflow(name: &str) -> String {
    let path = format!("{}/.github/workflows/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// Leading-space count of a line.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// `./x` and `x` name the same file.
fn norm(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

/// The job that runs `gh release create`, without comment lines: a
/// commented-out step must not count as present.
fn publishing_job(workflow: &str) -> Option<String> {
    let lines: Vec<&str> = workflow
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect();
    let jobs_at = lines.iter().position(|l| l.trim_end() == "jobs:")?;
    let mut jobs: Vec<Vec<&str>> = Vec::new();
    for line in &lines[jobs_at + 1..] {
        if indent(line) == 2 && line.trim_end().ends_with(':') {
            jobs.push(Vec::new());
        }
        if let Some(job) = jobs.last_mut() {
            job.push(line);
        }
    }
    jobs.into_iter()
        .find(|job| job.iter().any(|l| l.contains("gh release create")))
        .map(|job| job.join("\n"))
}

/// The job's steps, split at the indentation of its first `- ` item.
fn job_steps(job: &str) -> Vec<String> {
    let lines: Vec<&str> = job.lines().collect();
    let Some(start) = lines.iter().position(|l| l.trim_end().ends_with("steps:")) else {
        return Vec::new();
    };
    let Some(step_indent) = lines[start + 1..]
        .iter()
        .find(|l| l.trim_start().starts_with("- "))
        .map(|l| indent(l))
    else {
        return Vec::new();
    };
    let mut steps: Vec<String> = Vec::new();
    for line in &lines[start + 1..] {
        if indent(line) == step_indent && line.trim_start().starts_with("- ") {
            steps.push(String::new());
        }
        if let Some(step) = steps.last_mut() {
            step.push_str(line);
            step.push('\n');
        }
    }
    steps
}

/// Shell commands in a step, with `\` continuations joined. A one-line
/// `run: cmd` counts as the command `cmd`.
fn commands(step: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for line in step.lines() {
        let t = line.trim();
        let t = t.strip_prefix("- ").unwrap_or(t);
        let t = t.strip_prefix("run:").map_or(t, str::trim_start);
        if let Some(head) = t.strip_suffix('\\') {
            cur.push_str(head.trim_end());
            cur.push(' ');
        } else {
            cur.push_str(t);
            out.push(std::mem::take(&mut cur));
        }
    }
    out
}

/// Whitespace tokens with surrounding quotes removed.
fn tokens(command: &str) -> Vec<String> {
    command
        .split_whitespace()
        .map(|t| t.trim_matches(|c| c == '"' || c == '\'').to_string())
        .collect()
}

/// The value of `--flag v` or `--flag=v`.
fn flag(tokens: &[String], name: &str) -> Option<String> {
    let eq = format!("{name}=");
    tokens.iter().enumerate().find_map(|(i, t)| {
        if t == name {
            tokens.get(i + 1).cloned()
        } else {
            t.strip_prefix(&eq).map(str::to_string)
        }
    })
}

/// A `*`-only glob, `*` matching within one path component.
fn glob_matches(pattern: &str, path: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == path,
        Some((head, rest)) => {
            let Some(tail) = path.strip_prefix(head) else {
                return false;
            };
            (0..=tail.len())
                .take_while(|&i| !tail[..i].contains('/'))
                .filter(|&i| tail.is_char_boundary(i))
                .any(|i| glob_matches(rest, &tail[i..]))
        }
    }
}

/// The first step at or after `from` with a command starting `prefix`.
fn find_command(steps: &[String], from: usize, prefix: &str) -> Option<(usize, Vec<String>)> {
    steps.iter().enumerate().skip(from).find_map(|(i, step)| {
        commands(step)
            .into_iter()
            .find(|c| c.starts_with(prefix))
            .map(|c| (i, tokens(&c)))
    })
}

/// Every reason the publishing job would publish a release Scorecard
/// cannot see as signed and attested. Empty means none.
fn signing_gaps(workflow: &str) -> Vec<String> {
    let mut gaps: Vec<String> = Vec::new();
    let Some(job) = publishing_job(workflow) else {
        return vec!["no job runs `gh release create`".to_string()];
    };
    if !job.lines().any(|l| l.trim() == "id-token: write") {
        gaps.push("the publishing job does not grant `id-token: write`".to_string());
    }
    let steps = job_steps(&job);
    let Some(publish) = steps.iter().position(|s| s.contains("gh release create")) else {
        return vec!["the publishing job has no `gh release create` step".to_string()];
    };
    // Every `gh release` call that sends files: the create, and the
    // upload a rerun takes instead.
    let senders: Vec<(String, Vec<String>)> = commands(&steps[publish])
        .into_iter()
        .filter(|c| c.starts_with("gh release create") || c.starts_with("gh release upload"))
        .map(|c| {
            let t = tokens(&c);
            (t[..3].join(" "), t)
        })
        .collect();
    let sent = |path: &str, what: &str, gaps: &mut Vec<String>| {
        for (call, args) in &senders {
            if !args.iter().any(|a| glob_matches(norm(a), norm(path))) {
                gaps.push(format!("`{call}` does not send {what} `{path}`"));
            }
        }
    };

    if find_command(&steps[..publish], 0, "sha256sum").is_none() {
        gaps.push(format!(
            "no step before the publish step writes {SIGNED_BLOB}"
        ));
    }
    sent(SIGNED_BLOB, "the checksum list", &mut gaps);

    // (a) The signature over the checksum list.
    match find_command(&steps[..publish], 0, "cosign sign-blob") {
        None => gaps.push("no step before the publish step runs `cosign sign-blob`".to_string()),
        Some((at, sign)) => {
            let bundle = flag(&sign, "--bundle").unwrap_or_default();
            if sign.last().map(|s| norm(s)) != Some(SIGNED_BLOB) {
                gaps.push(format!("`cosign sign-blob` does not sign {SIGNED_BLOB}"));
            }
            if !bundle.ends_with(".sigstore.json") {
                gaps.push(format!(
                    "the signature bundle `{bundle}` is not named *.sigstore.json"
                ));
            }
            if !SCORECARD_SIGNATURE_SUFFIXES
                .iter()
                .any(|s| bundle.ends_with(s))
            {
                gaps.push(format!(
                    "Scorecard does not count `{bundle}` as a signature"
                ));
            }
            sent(&bundle, "the signature", &mut gaps);
            match find_command(&steps[..publish], at, "cosign verify-blob") {
                None => gaps.push(
                    "no step between signing and publishing runs `cosign verify-blob`".to_string(),
                ),
                Some((_, verify)) => {
                    if flag(&verify, "--bundle").as_deref() != Some(bundle.as_str())
                        || verify.last().map(|s| norm(s)) != Some(SIGNED_BLOB)
                    {
                        gaps.push(format!(
                            "`cosign verify-blob` does not check `{bundle}` against {SIGNED_BLOB}"
                        ));
                    }
                    if flag(&verify, "--certificate-oidc-issuer").as_deref() != Some(ACTIONS_ISSUER)
                    {
                        gaps.push(format!(
                            "`cosign verify-blob` does not pin the issuer to {ACTIONS_ISSUER}"
                        ));
                    }
                    if flag(&verify, "--certificate-identity").is_none()
                        && flag(&verify, "--certificate-identity-regexp").is_none()
                    {
                        gaps.push(
                            "`cosign verify-blob` does not pin the signing identity".to_string(),
                        );
                    }
                }
            }
        }
    }

    // (b) The provenance file.
    let attest = steps[..publish]
        .iter()
        .position(|s| s.contains("actions/attest-build-provenance@"));
    let attest_id = attest.and_then(|i| {
        steps[i].lines().find_map(|l| {
            l.trim()
                .trim_start_matches("- ")
                .strip_prefix("id:")
                .map(|v| v.trim().to_string())
        })
    });
    match (attest, attest_id) {
        (None, _) => gaps.push("no step runs actions/attest-build-provenance".to_string()),
        (Some(_), None) => gaps.push(
            "the attest-build-provenance step has no `id:`, so its bundle-path cannot be read"
                .to_string(),
        ),
        (Some(at), Some(id)) => {
            if !steps[at].lines().any(|l| norm(l.trim()) == SIGNED_BLOB) {
                gaps.push(format!(
                    "the attestation's subject-path does not name {SIGNED_BLOB}"
                ));
            }
            let output = format!("steps.{id}.outputs.bundle-path");
            let copy = steps[..publish]
                .iter()
                .enumerate()
                .skip(at + 1)
                .find(|(_, s)| s.contains(&output));
            let target = copy.and_then(|(_, s)| {
                commands(s)
                    .iter()
                    .filter(|c| c.starts_with("cp "))
                    .flat_map(|c| tokens(c))
                    .find(|t| t.ends_with(".intoto.jsonl") || t.ends_with(".intoto.json"))
            });
            match (copy, target) {
                (None, _) => gaps.push(format!("no step after the attestation reads `{output}`")),
                (Some(_), None) => gaps.push(format!(
                    "the step reading `{output}` does not `cp` it to an *.intoto.jsonl file"
                )),
                (Some((copied_at, _)), Some(path)) => {
                    if !SCORECARD_PROVENANCE_SUFFIXES
                        .iter()
                        .any(|s| path.ends_with(s))
                    {
                        gaps.push(format!("Scorecard does not count `{path}` as provenance"));
                    }
                    sent(&path, "the provenance", &mut gaps);
                    let verified = steps[copied_at..publish]
                        .iter()
                        .flat_map(|s| commands(s))
                        .filter(|c| c.contains("gh attestation verify"))
                        .any(|c| flag(&tokens(&c), "--bundle").as_deref() == Some(path.as_str()));
                    if !verified {
                        gaps.push(format!(
                            "no step between the copy and the publish step runs \
                             `gh attestation verify --bundle {path}`"
                        ));
                    }
                }
            }
        }
    }
    gaps
}

#[test]
fn the_release_publishes_a_verified_signature_and_provenance_asset() {
    let gaps = signing_gaps(&workflow("release.yml"));
    assert!(
        gaps.is_empty(),
        "release.yml would publish a release that Scorecard's Signed-Releases \
         check scores 0:\n  {}",
        gaps.join("\n  ")
    );
}

// ---------------------------------------------------------------------
// Fixtures: the checker against each fault it exists to catch.
// ---------------------------------------------------------------------

/// A minimal publishing job that satisfies every rule.
const GOOD: &str = r#"name: Release
jobs:
  vsix:
    steps:
      - run: make
  publish:
    permissions:
      contents: write
      id-token: write
      attestations: write
    steps:
      - name: Write the checksums
        run: sha256sum -- ./x-*.tar.gz > SHA256SUMS.txt
      - name: Attest build provenance
        id: provenance
        uses: actions/attest-build-provenance@0000000000000000000000000000000000000000 # v4.2.2
        with:
          subject-path: |
            ./x-*.tar.gz
            SHA256SUMS.txt
      - name: Keep the provenance bundle
        env:
          BUNDLE: ${{ steps.provenance.outputs.bundle-path }}
        run: cp "$BUNDLE" "x-${GITHUB_REF_NAME}.intoto.jsonl"
      - name: Sign the checksum list
        run: |
          cosign sign-blob --yes \
            --bundle SHA256SUMS.txt.sigstore.json \
            SHA256SUMS.txt
      - name: Verify
        run: |
          cosign verify-blob \
            --bundle SHA256SUMS.txt.sigstore.json \
            --certificate-identity "https://github.com/x" \
            --certificate-oidc-issuer https://token.actions.githubusercontent.com \
            SHA256SUMS.txt
          gh attestation verify SHA256SUMS.txt \
            --bundle "x-${GITHUB_REF_NAME}.intoto.jsonl" --repo x/y
      - name: Create release
        run: |
          if gh release view "$T"; then
            gh release upload "$T" --clobber \
              ./x-*.tar.gz ./SHA256SUMS.txt ./SHA256SUMS.txt.sigstore.json ./*.intoto.jsonl
          else
            gh release create "$T" \
              ./x-*.tar.gz ./SHA256SUMS.txt ./SHA256SUMS.txt.sigstore.json ./*.intoto.jsonl
          fi
"#;

/// `GOOD` with one exact substring replaced `n` times. Panics if the
/// count differs, so a fault that never landed cannot pass as caught.
fn planted(from: &str, to: &str, n: usize) -> String {
    assert_eq!(GOOD.matches(from).count(), n, "fixture fault `{from}`");
    GOOD.replace(from, to)
}

fn assert_caught(fault: &str, workflow: &str, expect: &str) {
    let gaps = signing_gaps(workflow);
    assert!(
        gaps.iter().any(|g| g.contains(expect)),
        "fault `{fault}` was not reported as `{expect}`; got {gaps:?}"
    );
}

#[test]
fn the_good_fixture_has_no_gaps() {
    assert_eq!(signing_gaps(GOOD), Vec::<String>::new());
}

#[test]
fn a_release_that_does_not_upload_the_signature_is_caught() {
    assert_caught(
        "signature dropped from both calls",
        &planted(" ./SHA256SUMS.txt.sigstore.json", "", 2),
        "does not send the signature",
    );
}

#[test]
fn a_rerun_that_does_not_upload_the_provenance_is_caught() {
    let rerun_only = GOOD.replacen(" ./*.intoto.jsonl", "", 1);
    assert_ne!(rerun_only, GOOD);
    assert_caught(
        "provenance dropped from the rerun upload",
        &rerun_only,
        "`gh release upload` does not send the provenance",
    );
}

#[test]
fn a_signature_scorecard_does_not_recognize_is_caught() {
    assert_caught(
        "signature misnamed",
        &planted(
            "SHA256SUMS.txt.sigstore.json",
            "SHA256SUMS.txt.cosign.json",
            4,
        ),
        "Scorecard does not count",
    );
}

#[test]
fn a_provenance_file_scorecard_does_not_recognize_is_caught() {
    assert_caught(
        "provenance misnamed",
        &planted(".intoto.jsonl", ".intoto.json", 4),
        "Scorecard does not count",
    );
}

#[test]
fn a_release_without_signing_is_caught() {
    assert_caught(
        "sign step removed",
        &planted("cosign sign-blob --yes", "true", 1),
        "runs `cosign sign-blob`",
    );
}

#[test]
fn a_commented_out_signing_step_is_caught() {
    assert_caught(
        "sign step commented out",
        &planted("cosign sign-blob --yes", "# cosign sign-blob --yes", 1),
        "runs `cosign sign-blob`",
    );
}

#[test]
fn a_signature_that_is_never_verified_is_caught() {
    assert_caught(
        "verify-blob removed",
        &planted("cosign verify-blob \\", "true \\", 1),
        "runs `cosign verify-blob`",
    );
}

#[test]
fn a_verification_that_accepts_any_issuer_is_caught() {
    assert_caught(
        "issuer dropped",
        &planted(
            "            --certificate-oidc-issuer https://token.actions.githubusercontent.com \\\n",
            "",
            1,
        ),
        "does not pin the issuer",
    );
}

#[test]
fn a_verification_that_accepts_any_identity_is_caught() {
    assert_caught(
        "identity dropped",
        &planted(
            "            --certificate-identity \"https://github.com/x\" \\\n",
            "",
            1,
        ),
        "does not pin the signing identity",
    );
}

#[test]
fn a_provenance_file_that_is_never_verified_is_caught() {
    assert_caught(
        "gh attestation verify removed",
        &planted("gh attestation verify SHA256SUMS.txt", "true", 1),
        "gh attestation verify --bundle",
    );
}

#[test]
fn a_provenance_step_without_an_id_is_caught() {
    assert_caught(
        "attest id removed",
        &planted("        id: provenance\n", "", 1),
        "has no `id:`",
    );
}

#[test]
fn a_job_without_an_oidc_grant_is_caught() {
    assert_caught(
        "id-token removed",
        &planted("      id-token: write\n", "", 1),
        "id-token: write",
    );
}

#[test]
fn a_release_without_a_checksum_list_is_caught() {
    assert_caught(
        "checksums removed",
        &planted("sha256sum -- ./x-*.tar.gz > SHA256SUMS.txt", "true", 1),
        "writes SHA256SUMS.txt",
    );
}

#[test]
fn a_one_line_run_is_read_as_its_command() {
    assert_eq!(
        commands("      - run: cp a b\n        run: |\n          x \\\n            y\n"),
        vec!["cp a b".to_string(), "|".to_string(), "x y".to_string()]
    );
}

#[test]
fn the_glob_matches_within_one_path_component() {
    assert!(
        glob_matches("*.intoto.jsonl", "x-${GITHUB_REF_NAME}.intoto.jsonl"),
        "`*` must match the tag variable inside a name"
    );
    assert!(
        glob_matches(
            "SHA256SUMS.txt.sigstore.json",
            "SHA256SUMS.txt.sigstore.json"
        ),
        "a pattern without `*` must match its own path"
    );
    assert!(
        !glob_matches("*.sig", "SHA256SUMS.txt.sigstore.json"),
        "a pattern must match the whole name, not a prefix of its suffix"
    );
    assert!(
        !glob_matches("*.jsonl", "dir/x.jsonl"),
        "`*` must not cross a `/`"
    );
}
