//! Gate: the wiki generator must produce a complete, link-correct
//! wiki tree from the in-repo docs (docs/ + README are the source of
//! truth; the GitHub wiki is a generated mirror).

mod common;

use std::process::Command;

#[test]
fn wiki_tree_is_generated_from_docs() {
    let root = env!("CARGO_MANIFEST_DIR");
    let out_scratch = common::Scratch::new("oslsp-wiki");
    let out = out_scratch.path().to_path_buf();

    let status = Command::new("python3")
        .arg(format!("{root}/scripts/build-wiki.py"))
        .arg(&out)
        .current_dir(root)
        .status();
    let status = status.expect(
        "python3 must be available: the wiki build is part of the suite \
         and a skipped test is a failed test",
    );
    assert!(status.success(), "build-wiki.py must succeed");

    for page in [
        "Home.md",
        "Admin-Guide.md",
        "Editor-Setup.md",
        "_Sidebar.md",
    ] {
        assert!(out.join(page).is_file(), "missing wiki page {page}");
    }

    let admin = std::fs::read_to_string(out.join("Admin-Guide.md")).unwrap();
    // the wiki renders the page title itself: leading H1 must be stripped
    assert!(!admin.starts_with("# "), "leading H1 must be stripped");
    // content survived
    assert!(
        admin.contains("opensipsPath"),
        "the Admin-Guide body must survive the H1 strip: {admin}"
    );

    let home = std::fs::read_to_string(out.join("Home.md")).unwrap();
    // inter-doc links must point at wiki pages, not repo-relative .md files
    assert!(
        !home.contains("docs/ADMIN.md"),
        "raw doc link leaked into wiki"
    );
    assert!(
        !home.contains("docs/EDITORS.md"),
        "raw docs/EDITORS.md link leaked into the wiki Home page: {home}"
    );
    assert!(home.contains("Admin-Guide"), "rewritten wiki link missing");

    let sidebar = std::fs::read_to_string(out.join("_Sidebar.md")).unwrap();
    assert!(
        sidebar.contains("Admin-Guide") && sidebar.contains("Editor-Setup"),
        "the sidebar must link the Admin-Guide and Editor-Setup pages: {sidebar}"
    );
    assert!(
        sidebar.contains("Getting-Started"),
        "the sidebar must link the Getting-Started page: {sidebar}"
    );
}
