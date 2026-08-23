//! Language Server Protocol implementation for the OpenSIPS routing
//! script language (`opensips.cfg`).
//!
//! Semantic truth stays in OpenSIPS itself: diagnostics come from a
//! real `opensips -C` run; completion/hover come from a documentation
//! catalog harvested out of an OpenSIPS source tree.
#![deny(missing_docs)]

/// Lexical analysis of cfg documents.
pub mod analyze;
/// Module documentation harvesting.
pub mod catalog;
pub mod cli;
/// `opensips -C` diagnostics.
pub mod diag;
/// Completion/hover/definition assembly.
pub mod logic;
pub mod memo;
/// The tower-lsp-server wiring.
pub mod server;
