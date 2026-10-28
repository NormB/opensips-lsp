//! The server's shared types are `Send + Sync` (Rust API Guidelines
//! C-SEND-SYNC): the transport runs handlers on several tasks at once, and
//! a check started by a notification runs on a task of its own.

use opensips_lsp::catalog::{CoreDocs, Item, ModuleDoc};
use opensips_lsp::diag::Diag;
use opensips_lsp::logic::IncludeGraph;
use opensips_lsp::memo::{AnalysisCache, DocAnalysis};
use opensips_lsp::server::{Backend, State};

fn is_send_sync<T: Send + Sync>() {}

#[test]
fn the_shared_types_are_send_and_sync() {
    is_send_sync::<Backend>();
    is_send_sync::<State>();
    is_send_sync::<AnalysisCache>();
    is_send_sync::<DocAnalysis>();
    is_send_sync::<IncludeGraph>();
    is_send_sync::<ModuleDoc>();
    is_send_sync::<CoreDocs>();
    is_send_sync::<Item>();
    is_send_sync::<Diag>();
}
