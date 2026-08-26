//! `include_file` is flattened, not lexed — and the analyzer must
//! follow the flattener rather than the lexer.
//!
//! `cfg_pp.c:mk_included_file_path` skips leading whitespace and then
//! matches `include_file` / `import_file` followed by a quoted path.
//! It runs over the raw lines BEFORE anything is lexed, so it knows
//! nothing about comments or strings: a directive that opens a line
//! fires even inside a `/* */` block, and one that does not open a
//! line never fires however plainly it is written.
//!
//! Reading it as code position — the lexer's idea — misses a file the
//! real parser opens and splices. Commenting a block out with `/* */`
//! does not stop its includes loading, and a server that cannot see
//! them reports every route they define as undefined.
//!
//! Rather than restate that rule, each shape is put to the real
//! binary: a config naming a file that does not exist, so "did it
//! fire" is exactly "did it try to open it".

mod common;

use opensips_lsp::analyze::includes;

/// The shapes, each with the directive in a different position.
/// `{}` is replaced by a path that does not exist.
const SHAPES: &[(&str, &str)] = &[
    (
        "plain",
        "log_level=2\ninclude_file \"{}\"\nroute {{ exit; }}\n",
    ),
    (
        "indented",
        "log_level=2\n    include_file \"{}\"\nroute {{ exit; }}\n",
    ),
    (
        "own line inside a block comment",
        "log_level=2\n/*\ninclude_file \"{}\"\n*/\nroute {{ exit; }}\n",
    ),
    (
        "indented inside a block comment",
        "log_level=2\n/*\n    include_file \"{}\"\n*/\nroute {{ exit; }}\n",
    ),
    (
        "own line inside a string",
        "log_level=2\nroute {{\n$var(x) = \"\ninclude_file \"{}\"\n\";\nexit;\n}}\n",
    ),
    (
        "same line as the comment opener",
        "log_level=2\n/* include_file \"{}\" */\nroute {{ exit; }}\n",
    ),
    (
        "after a hash comment",
        "log_level=2\n# include_file \"{}\"\nroute {{ exit; }}\n",
    ),
    (
        "inside a call on one line",
        "log_level=2\nroute {{\nxlog(\"include_file \\\"{}\\\"\");\nexit;\n}}\n",
    ),
    (
        "tail of a longer identifier",
        "log_level=2\nreinclude_file \"{}\"\nroute {{ exit; }}\n",
    ),
];

/// Whether the real binary tries to open the named file.
fn the_real_parser_opens_it(bin: &str, dir: &std::path::Path, tag: &str, cfg: &str) -> bool {
    let path = dir.join(format!("{tag}.cfg"));
    std::fs::write(&path, cfg).expect("fixture config");
    let out = std::process::Command::new(bin)
        .arg("-C")
        .arg("-f")
        .arg(&path)
        .output()
        .expect("the real binary runs");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    all.contains("failed to open")
}

#[test]
fn the_analyzer_sees_exactly_what_the_flattener_opens() {
    let bin = common::required_env("OPENSIPS_LSP_TEST_BIN");
    let dir = std::env::temp_dir().join(format!("oslsp-flatten-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("definitely-missing.inc");
    assert!(!missing.exists(), "the probe file must not exist");

    let mut disagreements: Vec<String> = Vec::new();
    let mut fired = 0usize;
    for (label, template) in SHAPES {
        let cfg = template.replace("{}", &missing.display().to_string());
        let tag: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        let real = the_real_parser_opens_it(&bin, &dir, &tag, &cfg);
        let seen = includes(&cfg)
            .iter()
            .any(|i| i.name.contains("definitely-missing"));
        if real {
            fired += 1;
        }
        if real != seen {
            disagreements.push(format!(
                "  {label}: the real parser {} the file, the analyzer {} it",
                if real { "OPENS" } else { "ignores" },
                if seen { "sees" } else { "misses" }
            ));
        }
    }

    // POSITIVE CONTROL: if no shape fired, every comparison would be
    // "neither" and the test would pass having proved nothing.
    assert!(
        fired >= 4,
        "only {fired} of {} shapes made the real parser open the file",
        SHAPES.len()
    );
    assert!(
        disagreements.is_empty(),
        "the analyzer and the real flattener disagree on {} shape(s):\n{}",
        disagreements.len(),
        disagreements.join("\n")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The rule stated directly, so a failure says which half broke
/// without needing the binary.
#[test]
fn a_directive_that_opens_a_line_fires_whatever_encloses_it() {
    // inside a block comment, on its own line
    let inc = includes("log_level=2\n/*\ninclude_file \"fires.inc\"\n*/\n");
    assert_eq!(
        inc.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["fires.inc"],
        "a block comment does not stop the flattener"
    );
    // indented, still the first token on its line
    let inc = includes("/*\n\timport_file 'also.inc'\n*/\n");
    assert_eq!(
        inc.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["also.inc"],
        "leading whitespace is skipped, and single quotes are accepted"
    );
}

#[test]
fn a_directive_that_does_not_open_a_line_never_fires() {
    for text in [
        "# include_file \"no.inc\"\n",
        "/* include_file \"no.inc\" */\n",
        "xlog(\"include_file \\\"no.inc\\\"\");\n",
        "reinclude_file \"no.inc\"\n",
        "$var(x) = 1; include_file \"no.inc\"\n",
    ] {
        assert!(
            includes(text).is_empty(),
            "must not fire, the directive does not open the line: {text:?}"
        );
    }
}
