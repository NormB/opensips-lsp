//! Memoization of the per-document analysis.
//!
//! The cache had a unit test that it returns the right answer, which
//! is not the property that matters: a version that recomputed on
//! every request would return the right answer too, pass every test,
//! and only show up as an editor that gets hot on a large config.
//! What is checked here is that the work happens ONCE per version.

use opensips_lsp::memo::{AnalysisCache, analysis_builds};

/// The build counter is process-global and `cargo test` runs these in
/// parallel threads, so a delta measured across one test would count
/// the others' work too.  Whoever is counting holds this.
static COUNTING: std::sync::Mutex<()> = std::sync::Mutex::new(());

const CFG: &str = "route[a] {\n    route(b);\n}\nroute[b] {\n    exit;\n}\n";

#[test]
fn repeated_requests_for_one_version_build_the_analysis_once() {
    let _counting = COUNTING.lock().unwrap_or_else(|e| e.into_inner());
    let cache = AnalysisCache::default();
    let before = analysis_builds();
    for _ in 0..10 {
        let a = cache.get_or_compute("file:///t.cfg", 1, CFG);
        assert_eq!(a.blocks.len(), 2);
    }
    assert_eq!(
        analysis_builds() - before,
        1,
        "ten requests for one version must build the analysis once"
    );
}

#[test]
fn a_new_version_builds_again_and_an_old_one_is_not_served() {
    let _counting = COUNTING.lock().unwrap_or_else(|e| e.into_inner());
    let cache = AnalysisCache::default();
    let before = analysis_builds();
    let v1 = cache.get_or_compute("file:///t.cfg", 1, CFG);
    let v2 = cache.get_or_compute("file:///t.cfg", 2, "route[only] {\n    exit;\n}\n");
    assert_eq!(
        analysis_builds() - before,
        2,
        "a second version is different text and must be built"
    );
    assert_eq!(v1.blocks.len(), 2);
    assert_eq!(v2.blocks.len(), 1, "the new version must not serve the old");
}

/// Two documents are two entries; one must not evict or answer for the
/// other.
#[test]
fn documents_do_not_share_an_entry() {
    let _counting = COUNTING.lock().unwrap_or_else(|e| e.into_inner());
    let cache = AnalysisCache::default();
    cache.get_or_compute("file:///a.cfg", 1, CFG);
    cache.get_or_compute("file:///b.cfg", 1, "route[only] {\n    exit;\n}\n");
    assert_eq!(cache.len(), 2);
    let before = analysis_builds();
    cache.get_or_compute("file:///a.cfg", 1, CFG);
    assert_eq!(
        analysis_builds() - before,
        0,
        "the first document's entry survived the second"
    );
    cache.evict("file:///a.cfg");
    assert_eq!(cache.len(), 1);
}
