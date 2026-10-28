//! No `too_many_arguments` suppression anywhere in the crate.
//!
//! A function that needs more parameters than clippy accepts is a
//! function whose inputs belong in a type, or that is re-reading state
//! it could hold.  Silencing the lint hides that rather than fixing
//! it, so the suppression is not allowed back in.
//!
//! The lint name is spelled in two pieces below so this file does not
//! match its own scan.

use std::path::PathBuf;

#[test]
fn no_source_silences_the_argument_count_lint() {
    let lint = format!("clippy::{}", "too_many_arguments");
    let root = env!("CARGO_MANIFEST_DIR");
    let mut stack = vec![
        PathBuf::from(root).join("src"),
        PathBuf::from(root).join("tests"),
    ];
    let mut scanned = 0;
    let mut offenders = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                scanned += 1;
                let text = std::fs::read_to_string(&path).unwrap();
                for (n, line) in text.lines().enumerate() {
                    if line.contains(&lint) {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    assert!(scanned > 10, "the scan found only {scanned} files");
    assert!(offenders.is_empty(), "suppressed: {offenders:?}");
}
