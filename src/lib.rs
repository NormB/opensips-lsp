//! Language Server Protocol implementation for the OpenSIPS routing
//! script language (`opensips.cfg`).
//!
//! Semantic truth stays in OpenSIPS itself: diagnostics come from a
//! real `opensips -C` run; completion/hover come from a documentation
//! catalog harvested out of an OpenSIPS source tree.
#![deny(clippy::unwrap_used)]

/// Lexical analysis of cfg documents.
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod analyze;
/// Module documentation harvesting.
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod catalog;
pub mod cli;
/// `opensips -C` diagnostics.
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod diag;
/// Whitespace formatting for cfg documents.
pub mod format;
/// Completion/hover/definition assembly.
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod logic;
pub mod memo;
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod outline;
/// The tower-lsp-server wiring.
// Still holds unwrap() calls that a later change replaces.
#[allow(clippy::unwrap_used)]
pub mod server;
