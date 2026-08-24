//! Per-document-version memoization of the scanner passes the hot
//! request handlers share (document symbols, folding, code lenses,
//! navigation).  Keyed by (URI, version): a new version replaces the
//! entry, so at most one analysis is retained per open document.

use dashmap::DashMap;
use std::sync::Arc;

use crate::analyze;

/// The memoized scanner output for one document version.
pub struct DocAnalysis {
    /// [`analyze::route_blocks`] of the text.
    pub blocks: Vec<analyze::Block>,
    /// [`analyze::route_refs`] of the text.
    pub refs: Vec<analyze::Located>,
}

/// Version-keyed cache: get-or-compute semantics, safe under
/// concurrent requests (a racing pair may compute twice; last insert
/// wins and both results are identical).
#[derive(Default)]
pub struct AnalysisCache {
    map: DashMap<String, (i32, Arc<DocAnalysis>)>,
}

/// Total [`DocAnalysis`] builds in this process — an instrumentation
/// seam for the memoization tests, and cheap enough to always keep.
///
/// Without it the memoization is only checkable by reading the code:
/// a change that recomputed on every request would leave every test
/// green and only show up as a hot editor.
static ANALYSIS_BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many [`DocAnalysis`] builds have run in this process.
pub fn analysis_builds() -> usize {
    ANALYSIS_BUILDS.load(std::sync::atomic::Ordering::Relaxed)
}

impl AnalysisCache {
    /// The analysis for `(uri, version)`, computed from `text` on the
    /// first request for this version.
    pub fn get_or_compute(&self, uri: &str, version: i32, text: &str) -> Arc<DocAnalysis> {
        if let Some(e) = self.map.get(uri)
            && e.0 == version
        {
            return e.1.clone();
        }
        ANALYSIS_BUILDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let a = Arc::new(DocAnalysis {
            blocks: analyze::route_blocks(text),
            refs: analyze::route_refs(text),
        });
        self.map.insert(uri.to_string(), (version, a.clone()));
        a
    }

    /// Drop a closed document's entry.
    pub fn evict(&self, uri: &str) {
        self.map.remove(uri);
    }

    /// Number of cached documents.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}
