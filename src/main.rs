//! `opensips-lsp`: the language server on standard input and output, or
//! `opensips-lsp check <files>` to run the analyzer (and `opensips -C` when a
//! binary is configured) from a shell.
#![deny(clippy::unwrap_used)]

use opensips_lsp::server::Backend;
use tower_lsp_server::{LspService, Server};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("check") {
        std::process::exit(opensips_lsp::cli::run_check(&args[2..]));
    }
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    // `analysisRoot` is not an LSP request: the client needs it before
    // a document has a language, which is exactly when the standard
    // document requests do not apply to it yet.
    let (service, socket) = LspService::build(Backend::new)
        .custom_method("opensips/analysisRoot", Backend::analysis_root)
        .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
