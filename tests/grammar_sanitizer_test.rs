//! The tree-sitter grammar ships C: the hand-written `src/scanner.c` and
//! the generated `src/parser.c`, which Neovim, Helix and Zed compile into
//! the editor.  CI builds both with AddressSanitizer and
//! UndefinedBehaviorSanitizer and runs the grammar's corpus tests and a
//! parse of every shipped and fixture `.cfg` through them
//! (`scripts/grammar-sanitize.sh check`), after proving that the
//! sanitizers catch a fault planted in a copy of `scanner.c`
//! (`scripts/grammar-sanitize.sh plant`).  These tests keep that job, its
//! inputs and the sanitizer settings from being dropped or weakened.

fn read(path: &str) -> String {
    std::fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// The text of one job of `ci.yml`, from its key to the next job.
fn job(ci: &str, name: &str) -> String {
    let (_, rest) = ci
        .split_once(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("ci.yml has no `{name}` job"));
    // the job ends where the next key at the jobs' own indent begins
    let mut text = String::new();
    for line in rest.lines() {
        let is_next_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim_start().starts_with('#')
            && line.trim_end().ends_with(':');
        if is_next_job {
            break;
        }
        text.push_str(line);
        text.push('\n');
    }
    text
}

#[test]
fn ci_plants_a_fault_then_checks_the_grammar_under_the_sanitizers() {
    let ci = read(".github/workflows/ci.yml");
    let job = job(&ci, "grammar-sanitizers");
    let plant = job
        .find("scripts/grammar-sanitize.sh plant tree-sitter-opensips")
        .expect("the job does not prove the sanitizers catch a planted fault");
    let check = job
        .find("scripts/grammar-sanitize.sh check tree-sitter-opensips")
        .expect("the job does not check the grammar");
    assert!(
        plant < check,
        "the planted fault must be caught before the check is trusted"
    );
    let check_line = job[check..].lines().next().unwrap_or("");
    for input in [
        "tests/grammar",
        "tree-sitter-opensips/test",
        "OPENSIPS_LSP_TEST_TREES",
    ] {
        assert!(
            check_line.contains(input),
            "the check does not parse the .cfg files under {input}"
        );
    }
    assert!(
        job.contains("run: scripts/proof-env.sh"),
        "the job does not provision the OpenSIPS trees whose .cfg files it parses"
    );
}

#[test]
fn ci_success_waits_for_the_sanitizer_job() {
    let ci = read(".github/workflows/ci.yml");
    let gate = job(&ci, "ci-success");
    let needs = gate
        .lines()
        .find(|l| l.trim_start().starts_with("needs:"))
        .expect("ci-success has no needs");
    assert!(
        needs.contains("grammar-sanitizers"),
        "ci-success does not wait for grammar-sanitizers: {needs}"
    );
}

/// Shell `text` without its comment lines, with each command's
/// backslash-continued lines joined into one.
fn commands_of(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        match line.strip_suffix('\\') {
            Some(head) => {
                current.push_str(head);
                current.push(' ');
            }
            None => {
                current.push_str(line);
                commands.push(std::mem::take(&mut current));
            }
        }
    }
    commands
}

fn script_commands() -> Vec<String> {
    commands_of(&read("scripts/grammar-sanitize.sh"))
}

#[test]
fn the_script_builds_both_c_files_with_both_sanitizers_and_halts_on_a_report() {
    let commands = script_commands();
    let build: Vec<&String> = commands
        .iter()
        .filter(|c| c.starts_with("\"$CC\" -shared"))
        .collect();
    assert_eq!(build.len(), 1, "expected one build of the grammar library");
    for needle in [
        "-fsanitize=address,undefined",
        "-fno-omit-frame-pointer",
        "src/parser.c",
        "src/scanner.c",
    ] {
        assert!(
            build[0].contains(needle),
            "the build lost {needle}: {}",
            build[0]
        );
    }
    let code = commands.join("\n");
    for needle in [
        "ASAN_OPTIONS=halt_on_error=1",
        "UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1",
    ] {
        assert!(code.contains(needle), "grammar-sanitize.sh lost {needle}");
    }
    let sanitized_runs = commands
        .iter()
        .filter(|c| c.contains("sanitized \"$ts\"") && c.contains("--lib-path"))
        .count();
    assert_eq!(
        sanitized_runs, 2,
        "the corpus tests and the parse must both run the sanitized library"
    );
}

#[test]
fn script_commands_joins_continuations_and_drops_comments() {
    let sample = "# -fsanitize=address\n\t\"$CC\" -shared -a \\\n\t\t-b\nx\n";
    assert_eq!(commands_of(sample), ["\"$CC\" -shared -a  -b", "x"]);
}

#[test]
fn job_reads_one_job_and_stops_at_the_next() {
    let ci = "jobs:\n  a:\n    steps:\n      - run: x\n\n  # note\n  b:\n    needs: [a]\n";
    assert_eq!(job(ci, "a"), "    steps:\n      - run: x\n\n  # note\n");
    assert_eq!(job(ci, "b"), "    needs: [a]\n");
}
