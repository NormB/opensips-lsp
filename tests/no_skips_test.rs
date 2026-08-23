//! No-skip gate: a skipped test is a failed test in this repo.
//!
//! A suite that quietly opts out when a source tree or a binary is
//! missing reports green while proving nothing, and that is the worst
//! failure mode available to a test suite — it is invisible.  The
//! proof environment is provisioned by `scripts/proof-env.sh`, which
//! CI runs too, so there is never a legitimate reason to opt out.
//!
//! Two mechanical rules keep it that way:
//!
//! 1. no test announces that it is opting out;
//! 2. no test reads a proof variable directly — they all go through
//!    `common::required_env`, which panics when it is unset.
//!
//! This file is excluded from its own scan: it has to name the thing
//! it forbids in order to look for it.

use std::collections::BTreeSet;

#[test]
fn no_test_opts_out_of_the_suite() {
    let dir = format!("{}/tests", env!("CARGO_MANIFEST_DIR"));
    let me = "no_skips_test.rs";

    let mut scanned = BTreeSet::new();
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("tests/ is readable") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().is_none_or(|e| e != "rs") || name == me {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        scanned.insert(name.clone());

        for (n, line) in text.lines().enumerate() {
            // rule 1: an opt-out announcement
            if line.contains("\"SKIP") {
                offenders.push(format!(
                    "{name}:{} announces a skip: {}",
                    n + 1,
                    line.trim()
                ));
            }
            // rule 2: a proof variable read outside required_env, which
            // is how a test quietly makes itself optional again
            if line.contains("env::var(\"OPENSIPS_LSP_TEST") {
                offenders.push(format!(
                    "{name}:{} reads a proof variable directly; use \
                     common::required_env so a missing environment fails \
                     loudly: {}",
                    n + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        scanned.len() > 10,
        "only {} test files scanned — the scan is not seeing the suite",
        scanned.len()
    );
    assert!(
        offenders.is_empty(),
        "tests may not opt themselves out:\n  {}",
        offenders.join("\n  ")
    );
}
