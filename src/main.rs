use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("check") {
        std::process::exit(opensips_lsp::cli::run_check(&args[2..]));
    }
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(opensips_lsp::server::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
