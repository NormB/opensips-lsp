//! Example paths in documentation.
//!
//! An absolute home-directory example written into GETTING_STARTED
//! was caught by the PII guard, which reads it as a real home path —
//! rightly, because it cannot tell one home directory from another.
//! Nothing checks example paths at the point of writing them, and the
//! guard scans tracked files generally rather than documentation
//! specifically, so the failure arrives as "personal infrastructure
//! leaked" rather than "use a neutral example".

fn docs() -> Vec<(String, String)> {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut out = Vec::new();
    for dir in ["docs", "client"] {
        let Ok(entries) = std::fs::read_dir(format!("{root}/{dir}")) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "md")
                && let Ok(t) = std::fs::read_to_string(&p)
            {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), t));
            }
        }
    }
    out
}

#[test]
fn no_documentation_example_uses_a_home_directory() {
    let pages = docs();
    // POSITIVE CONTROL: no pages read would satisfy every absence.
    assert!(pages.len() >= 3, "only {} page(s) read", pages.len());

    let mut bad: Vec<String> = Vec::new();
    for (name, text) in &pages {
        for (i, line) in text.lines().enumerate() {
            // `~/.local/bin` and `~/.config/...` are the portable
            // conventions and are correct in documentation. What is
            // wrong is an absolute `/home/<someone>` path: it names
            // one machine's layout, and the guard reads it as a real
            // home directory because it cannot tell whose it is.
            if line.contains("/home/") {
                bad.push(format!("  {name}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "documentation examples must use a neutral path (`/opt/src/...`), not a home \
         directory — the PII guard cannot tell an example home from a real one:\n{}",
        bad.join("\n")
    );
}

/// The guard that caught it must actually be looking at these files.
///
/// It scans `git ls-files`, so a documentation page that is untracked
/// — newly written, not yet added — is invisible to it. That is
/// precisely the moment the example path is written.
#[test]
fn the_personal_infrastructure_guard_sees_the_documentation() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    assert!(out.status.success(), "git ls-files failed");
    let tracked: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();

    let pages: Vec<String> = docs().into_iter().map(|(n, _)| n).collect();
    assert!(pages.len() >= 3, "control: {} page(s)", pages.len());
    let unseen: Vec<&String> = pages
        .iter()
        .filter(|n| !tracked.iter().any(|t| t.ends_with(n.as_str())))
        .collect();
    assert!(
        unseen.is_empty(),
        "documentation the guard cannot see because it is untracked: {unseen:?}"
    );
}

/// The `json`-fenced blocks that are SETTINGS examples.
///
/// Not every such block is an object to paste into `settings.json`:
/// `EDITORS.md` shows an LSP exchange in one, annotated with `-->`
/// and `<--`. It is a transcript, not malformed JSON, and a gate that
/// demanded it parse would be demanding the documentation be worse.
fn settings_examples() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (name, text) in docs() {
        let mut in_json = false;
        let mut buf = String::new();
        for line in text.lines() {
            if line.starts_with("```json") {
                in_json = true;
                buf.clear();
                continue;
            }
            if in_json && line.starts_with("```") {
                in_json = false;
                let body = buf.trim().to_string();
                let is_transcript = body.contains("-->") || body.contains("<--");
                if !is_transcript && body.starts_with('{') {
                    out.push((name.clone(), body));
                }
                continue;
            }
            if in_json {
                buf.push_str(line);
                buf.push('\n');
            }
        }
    }
    out
}

/// Every settings example in the documentation must parse.
///
/// A settings example is copied verbatim into `settings.json`. One
/// that does not parse leaves the reader with a broken file and no
/// clue which line did it.
#[test]
fn every_settings_example_parses() {
    let examples = settings_examples();
    // POSITIVE CONTROL: none found would satisfy this without
    // parsing anything.
    assert!(
        examples.len() >= 5,
        "only {} example(s) found",
        examples.len()
    );
    let bad: Vec<String> = examples
        .iter()
        .filter(|(_, body)| serde_json::from_str::<serde_json::Value>(body).is_err())
        .map(|(name, body)| format!("  {name}: {}", body.replace('\n', " ")))
        .collect();
    assert!(
        bad.is_empty(),
        "examples that do not parse:\n{}",
        bad.join("\n")
    );
}

/// A settings example must name a setting that exists.
#[test]
fn every_settings_example_names_a_real_setting() {
    let pkg: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/client/package.json"))
            .unwrap(),
    )
    .unwrap();
    let known: Vec<String> = pkg["contributes"]["configuration"]["properties"]
        .as_object()
        .expect("properties")
        .keys()
        .cloned()
        .collect();
    let bare: Vec<&str> = known
        .iter()
        .filter_map(|k| k.split_once('.').map(|(_, r)| r))
        .collect();

    let mut checked = 0usize;
    let mut ghosts: Vec<String> = Vec::new();
    for (name, body) in settings_examples() {
        let Ok(serde_json::Value::Object(o)) = serde_json::from_str(&body) else {
            continue;
        };
        for k in o.keys() {
            // Only our own namespace can be judged here. The editor
            // pages show `files.associations`, `languageserver`,
            // `clients`, `file_types` and `lsp` — VS Code's, coc's,
            // Helix's and Zed's settings, correct as written and none
            // of this extension's business. Bare-key examples are
            // initializationOptions and are covered by the ADMIN
            // docs-parity gate.
            let Some(stem) = k.strip_prefix("opensipsLsp.") else {
                continue;
            };
            checked += 1;
            if !known.contains(k) && !bare.contains(&stem) {
                ghosts.push(format!("  {name}: {k}"));
            }
        }
    }
    assert!(
        checked >= 3,
        "only {checked} `opensipsLsp.` setting(s) named in examples"
    );
    assert!(
        ghosts.is_empty(),
        "examples naming settings the extension does not offer:\n{}",
        ghosts.join("\n")
    );
}

/// The personal-infrastructure guard must look at the tests too.
///
/// It caught this very file: the doc comment quoted a home path as an
/// example of what not to write, and the guard cannot tell an example
/// of a bad path from a bad path. That is the guard being right. It
/// only works because tests are in its scan, which nothing asserted.
#[test]
fn the_guard_scans_the_tests_as_well_as_the_sources() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out = std::process::Command::new("git")
        .args(["ls-files"])
        .current_dir(root)
        .output()
        .expect("git ls-files");
    let tracked: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    let tests: Vec<&String> = tracked.iter().filter(|t| t.starts_with("tests/")).collect();
    assert!(
        tests.len() >= 20,
        "only {} test file(s) tracked — the guard would not see the rest",
        tests.len()
    );
    assert!(
        tests.iter().any(|t| t.ends_with("docs_examples_test.rs")),
        "this file must be tracked, or the guard cannot see it"
    );
}
