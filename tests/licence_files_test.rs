//! The licence texts sit in the repository root as `LICENSE-APACHE` and
//! `LICENSE-MIT`, and again in `LICENSES/` named by SPDX identifier, the
//! layout of the REUSE specification.  The OpenSSF Best Practices site
//! looks for a `LICENSE` or `COPYING` file or a `LICENSES/` directory and
//! does not recognise the two root files, so `LICENSES/` is what it finds.
//! The copies must stay the same texts, and `LICENSES/` must hold no
//! licence the root does not.

use std::path::Path;

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
