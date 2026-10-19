//! Scratch paths are removed however a test ends, and no test makes one
//! without the guard that removes it.

mod common;

use common::Scratch;

#[test]
fn a_scratch_directory_is_removed_when_the_test_passes() {
    let path = {
        let scratch = Scratch::new("oslsp-scratch-pass");
        std::fs::create_dir_all(scratch.path().join("sub")).unwrap();
        std::fs::write(scratch.path().join("sub/f.cfg"), "route {}\n").unwrap();
        // POSITIVE CONTROL: there was something to remove
        assert!(scratch.path().join("sub/f.cfg").exists());
        scratch.path().to_path_buf()
    };
    assert!(!path.exists(), "{} survived its guard", path.display());
}

#[test]
fn a_scratch_directory_is_removed_when_the_test_fails() {
    let mut seen = None;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let scratch = Scratch::new("oslsp-scratch-fail");
        std::fs::create_dir_all(scratch.path()).unwrap();
        std::fs::write(scratch.path().join("f.cfg"), "route {}\n").unwrap();
        seen = Some(scratch.path().to_path_buf());
        panic!("a failing assertion");
    }));
    assert!(outcome.is_err(), "the closure was meant to panic");
    let path = seen.expect("the scratch path was recorded");
    assert!(!path.exists(), "{} survived a failing test", path.display());
}

#[test]
fn a_scratch_file_is_removed_too() {
    let path = {
        let scratch = Scratch::new("oslsp-scratch-file");
        std::fs::write(scratch.path(), "x").unwrap();
        assert!(scratch.path().is_file());
        scratch.path().to_path_buf()
    };
    assert!(!path.exists(), "{} survived its guard", path.display());
}

/// Two live guards on one path are refused; once the first is gone the
/// name is free again.
#[test]
fn a_path_held_by_a_live_guard_cannot_be_taken_again() {
    let first = Scratch::new("oslsp-scratch-twice");
    std::fs::create_dir_all(first.path()).unwrap();
    std::fs::write(first.path().join("fixture.cfg"), "route {}\n").unwrap();
    let second = std::panic::catch_unwind(|| Scratch::new("oslsp-scratch-twice"));
    assert!(
        second.is_err(),
        "a second live guard on the same path was allowed"
    );
    assert!(
        first.path().join("fixture.cfg").exists(),
        "the refused guard must not have cleared the first guard's files"
    );
    drop(first);
    let again = Scratch::new("oslsp-scratch-twice");
    assert!(!again.path().exists());
}

/// Every scratch path in the suite comes from `common::Scratch`.
///
/// A path built directly under a temp directory — the system's, or
/// Cargo's `target/tmp` — is cleaned up by whatever the test does last,
/// which a failing assertion skips. The needles are split so this file
/// does not match itself.
#[test]
fn no_test_builds_a_temp_path_without_the_guard() {
    let needles = [concat!("temp", "_dir("), concat!("CARGO_TARGET", "_TMPDIR")];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                if p != root.join("common") {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|e| e == "rs") {
                files.push(p);
            }
        }
    }
    // POSITIVE CONTROL: the walk found the suite
    assert!(files.len() > 50, "only {} test files found", files.len());
    let mut offenders: Vec<String> = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (i, line) in text.lines().enumerate() {
            if needles.iter().any(|n| line.contains(n)) {
                offenders.push(format!(
                    "{}:{}",
                    f.strip_prefix(&root).unwrap().display(),
                    i + 1
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} temp paths built without common::Scratch, so a failing test leaks them: {offenders:?}",
        offenders.len()
    );
}
