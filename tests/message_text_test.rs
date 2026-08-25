//! User-facing message strings, checked as text.
//!
//! A message assembled across source lines is written with a
//! backslash continuation, which swallows the newline AND the leading
//! indentation of the next line. Joining such a literal onto one
//! physical line without removing that indentation leaves the run of
//! spaces embedded in the message — it still compiles, still passes
//! every behavioural test, and reaches the user with a gap in the
//! middle of a sentence.
//!
//! Three had already reached `main` that way when this was written:
//! two in `opensips-lsp` and one in its sibling, all in strings shown to the
//! user through `show_message` or `log_message`.

/// String literals in the server sources, minus comment lines.
fn literals() -> Vec<(String, usize, String)> {
    let mut out = Vec::new();
    let re = regex::Regex::new(r#""((?:[^"\\]|\\.)*)""#).expect("literal regex");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir("src")
        .expect("src/ is readable")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("source is readable");
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for c in re.captures_iter(line) {
                out.push((
                    f.file_name().unwrap().to_string_lossy().into_owned(),
                    i + 1,
                    c[1].to_string(),
                ));
            }
        }
    }
    out
}

#[test]
fn no_message_carries_a_run_of_spaces() {
    let lits = literals();
    // POSITIVE CONTROL: a regex that stopped matching would find
    // nothing to complain about and pass while proving nothing.
    assert!(
        lits.len() > 100,
        "only {} string literals found in src/ — the scan has regressed",
        lits.len()
    );

    let mut bad: Vec<String> = Vec::new();
    for (file, line, lit) in &lits {
        if lit.contains("   ") {
            bad.push(format!("  {file}:{line}: {lit:?}"));
        }
    }
    assert!(
        bad.is_empty(),
        "{} string literal(s) contain a run of three or more spaces — \
         a continuation was joined without stripping its indentation:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// The specific messages the server shows the user must read as
/// sentences: one space after the dash, none doubled.
#[test]
fn the_user_facing_warnings_read_as_sentences() {
    let lits = literals();
    let mut checked = 0usize;
    for (file, line, lit) in &lits {
        if !lit.starts_with("opensips-lsp: ") {
            continue;
        }
        checked += 1;
        assert!(
            !lit.contains("  "),
            "{file}:{line}: message has a doubled space: {lit:?}"
        );
        assert!(
            !lit.contains(" \n"),
            "{file}:{line}: message has a trailing space before a newline: {lit:?}"
        );
    }
    // POSITIVE CONTROL: these are found by prefix, and a rename of the
    // server would silently empty the loop.
    assert!(
        checked >= 3,
        "only {checked} 'opensips-lsp: ' messages found — the prefix has changed"
    );
    eprintln!("{checked} user-facing messages checked");
}
