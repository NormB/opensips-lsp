//! The licence texts sit in the repository root as `LICENSE-APACHE` and
//! `LICENSE-MIT`, and again in `LICENSES/` named by SPDX identifier, the
//! layout of the REUSE specification.  The copies must stay the same
//! texts, and `LICENSES/` must hold no licence the root does not.
//!
//! The OpenSSF Best Practices site finds a license file only by its name:
//! `app/lib/repo_files_examine_detective.rb` accepts a root file matching
//! `/\A([A-Za-z0-9]+-)?(license|copying)(\.md|\.txt)?\Z/i` of at least
//! 40 bytes.  A prefix is allowed and a suffix is not, so `LICENSE-MIT` and
//! `LICENSE-APACHE` never match.  `DUAL-LICENSE` matches it.  A root file
//! named exactly `LICENSE` would also match, but GitHub's license detection
//! (licensee) would then read that file first and report the dual-license
//! notice as NOASSERTION instead of reading `LICENSE-APACHE`.

use std::path::Path;

use regex::Regex;

/// Each file in `LICENSES/` and the root file it copies.
const COPIES: [(&str, &str); 2] = [
    ("Apache-2.0.txt", "LICENSE-APACHE"),
    ("MIT.txt", "LICENSE-MIT"),
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn each_licence_in_licenses_is_the_root_text() {
    for (copy, original) in COPIES {
        let copy_path = root().join("LICENSES").join(copy);
        let copied = std::fs::read(&copy_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", copy_path.display()));
        let text = std::fs::read(root().join(original)).expect("read the root licence");
        assert!(copied == text, "LICENSES/{copy} differs from {original}");
    }
}

#[test]
fn licenses_holds_only_the_two_licences() {
    let dir = root().join("LICENSES");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("list {}: {e}", dir.display()))
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["Apache-2.0.txt", "MIT.txt"]);
}

/// The smallest file the Best Practices site counts as a license file.
const NONTRIVIAL_MIN_SIZE: u64 = 40;

/// The names GitHub's license detection reads before `LICENSE-APACHE`.
const PREFERRED_BY_LICENSEE: [&str; 4] = ["LICENSE", "LICENSE.md", "LICENSE.txt", "COPYING"];

/// Whether the Best Practices site would take `name` as a license file.
fn best_practices_license_name(name: &str) -> bool {
    Regex::new(r"(?i)\A([A-Za-z0-9]+-)?(license|copying)(\.md|\.txt)?\z")
        .expect("pattern")
        .is_match(name)
}

/// The names of the regular files in the repository root.
fn root_files() -> Vec<String> {
    std::fs::read_dir(root())
        .expect("list the repository root")
        .map(|entry| entry.expect("entry"))
        .filter(|entry| entry.file_type().expect("file type").is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn best_practices_name_rule_takes_a_prefix_but_not_a_suffix() {
    for name in ["LICENSE", "COPYING.txt", "license.md", "DUAL-LICENSE"] {
        assert!(best_practices_license_name(name), "{name} should match");
    }
    for name in [
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "LICENSES",
        "A-B-LICENSE",
        "LICENSE.rst",
    ] {
        assert!(
            !best_practices_license_name(name),
            "{name} should not match"
        );
    }
}

#[test]
fn a_root_file_is_one_best_practices_counts_as_a_license() {
    let found: Vec<String> = root_files()
        .into_iter()
        .filter(|name| best_practices_license_name(name))
        .filter(|name| {
            std::fs::metadata(root().join(name))
                .expect("metadata")
                .len()
                >= NONTRIVIAL_MIN_SIZE
        })
        .collect();
    assert!(
        !found.is_empty(),
        "no root file of {NONTRIVIAL_MIN_SIZE} bytes or more has a name the Best Practices site reads as a license"
    );
}

#[test]
fn no_root_file_takes_license_detection_from_the_license_texts() {
    let names = root_files();
    for name in PREFERRED_BY_LICENSEE {
        assert!(
            !names.iter().any(|n| n == name),
            "{name} in the root would be read by GitHub before LICENSE-APACHE"
        );
    }
}

#[test]
fn the_dual_license_notice_names_the_cargo_license() {
    let path = root().join("DUAL-LICENSE");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let tag = format!("SPDX-License-Identifier: {}", env!("CARGO_PKG_LICENSE"));
    assert!(
        text.lines().any(|line| line == tag),
        "DUAL-LICENSE has no line {tag:?}"
    );
    for name in ["LICENSE-APACHE", "LICENSE-MIT", "LICENSES/"] {
        assert!(text.contains(name), "DUAL-LICENSE does not name {name}");
    }
}
