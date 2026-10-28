//! Renaming a fragment in the editor rewrites every `include_file` /
//! `import_file` path that names it (`workspace/willRenameFiles`).
//!
//! The paths are resolved the way the server resolves an include,
//! against the including file's directory, and rewritten in the same
//! form: relative stays relative, absolute stays absolute. A moved file
//! that includes others by relative path gets those paths rewritten too,
//! so they still name the same files from its new directory.

use opensips_lsp::logic;

use opensips_lsp::logic::IncludeEdit;
use std::path::{Path, PathBuf};

fn configs(files: &[(&str, &str)]) -> Vec<(PathBuf, String)> {
    files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), t.to_string()))
        .collect()
}

fn edits(files: &[(&str, &str)], old: &str, new: &str) -> Vec<(String, u32, u32, u32, String)> {
    let mut out: Vec<(String, u32, u32, u32, String)> =
        logic::include_rename_edits(&configs(files), Path::new(old), Path::new(new))
            .into_iter()
            .map(
                |IncludeEdit {
                     file,
                     line,
                     col_start,
                     col_end,
                     new_text,
                 }| {
                    (
                        file.display().to_string(),
                        line,
                        col_start,
                        col_end,
                        new_text,
                    )
                },
            )
            .collect();
    out.sort();
    out
}

#[test]
fn a_rename_in_the_same_directory_rewrites_the_include() {
    let files = [
        (
            "/w/opensips.cfg",
            "include_file \"inc/a.cfg\"\nroute { exit; }\n",
        ),
        ("/w/inc/a.cfg", "route[A] { exit; }\n"),
    ];
    assert_eq!(
        edits(&files, "/w/inc/a.cfg", "/w/inc/b.cfg"),
        vec![("/w/opensips.cfg".into(), 0, 14, 23, "inc/b.cfg".into())]
    );
}

#[test]
fn a_move_into_another_directory_rewrites_the_include() {
    let files = [
        ("/w/opensips.cfg", "include_file \"inc/a.cfg\"\n"),
        ("/w/inc/a.cfg", "route[A] { exit; }\n"),
    ];
    assert_eq!(
        edits(&files, "/w/inc/a.cfg", "/w/lib/a.cfg"),
        vec![("/w/opensips.cfg".into(), 0, 14, 23, "lib/a.cfg".into())]
    );
}

/// The path in a nested include is relative to THAT file's directory,
/// and the quote it was written with is kept.
#[test]
fn a_relative_path_in_a_nested_include_is_rewritten_from_its_own_directory() {
    let files = [
        ("/w/opensips.cfg", "include_file \"inc/routes.cfg\"\n"),
        ("/w/inc/routes.cfg", "    import_file 'helpers/x.cfg'\n"),
        ("/w/inc/helpers/x.cfg", "route[X] { exit; }\n"),
    ];
    assert_eq!(
        edits(&files, "/w/inc/helpers/x.cfg", "/w/inc/shared/x.cfg"),
        vec![("/w/inc/routes.cfg".into(), 0, 17, 30, "shared/x.cfg".into())]
    );
}

/// The moved file's own relative includes keep naming the same files
/// from its new directory; the edit is made to the file as it is before
/// the rename, which is when the editor applies it.
#[test]
fn a_moved_file_keeps_its_own_relative_includes_pointing_at_the_same_files() {
    let files = [
        ("/w/opensips.cfg", "include_file \"inc/a.cfg\"\n"),
        (
            "/w/inc/a.cfg",
            "include_file \"b.cfg\"\ninclude_file \"/w/abs.cfg\"\n",
        ),
        ("/w/inc/b.cfg", "route[B] { exit; }\n"),
    ];
    assert_eq!(
        edits(&files, "/w/inc/a.cfg", "/w/lib/a.cfg"),
        vec![
            ("/w/inc/a.cfg".into(), 0, 14, 19, "../inc/b.cfg".into()),
            ("/w/opensips.cfg".into(), 0, 14, 23, "lib/a.cfg".into()),
        ]
    );
}

#[test]
fn an_absolute_include_stays_absolute() {
    let files = [("/w/opensips.cfg", "include_file \"/w/inc/a.cfg\"\n")];
    assert_eq!(
        edits(&files, "/w/inc/a.cfg", "/w/inc/b.cfg"),
        vec![("/w/opensips.cfg".into(), 0, 14, 26, "/w/inc/b.cfg".into())]
    );
}

/// The negative controls: an unrelated file, a file of the same name in
/// another directory, and the path written somewhere that is not an
/// include directive (OpenSIPS splices only a line that starts with it).
#[test]
fn nothing_else_is_rewritten() {
    let files = [
        (
            "/w/opensips.cfg",
            "include_file \"inc/a.cfg\"\nroute { xlog(\"include_file \\\"inc/z.cfg\\\"\"); }\n",
        ),
        ("/w/other/opensips.cfg", "include_file \"a.cfg\"\n"),
    ];
    assert!(
        edits(&files, "/w/unrelated.cfg", "/w/renamed.cfg").is_empty(),
        "no file includes /w/unrelated.cfg, so nothing is rewritten: {files:?}"
    );
    assert!(
        edits(&files, "/w/inc/z.cfg", "/w/inc/y.cfg").is_empty(),
        "inc/z.cfg appears only inside an xlog string, not an include_file directive: {files:?}"
    );
    assert_eq!(
        edits(&files, "/w/other/a.cfg", "/w/other/b.cfg"),
        vec![("/w/other/opensips.cfg".into(), 0, 14, 19, "b.cfg".into())]
    );
}
