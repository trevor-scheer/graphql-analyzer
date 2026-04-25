//! Standalone GraphQL Language Server binary
//!
//! This is a thin wrapper that starts the GraphQL LSP server.
//! For CLI usage with additional commands, use `graphql lsp` instead.

fn print_version() {
    let version = env!("CARGO_PKG_VERSION");
    let git_sha = option_env!("VERGEN_GIT_SHA").unwrap_or("unknown");
    let git_dirty = option_env!("VERGEN_GIT_DIRTY").unwrap_or("false");
    let dirty_suffix = if git_dirty == "true" { "-dirty" } else { "" };
    println!("graphql-lsp {version} ({git_sha}{dirty_suffix})");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        print_version();
        return;
    }

    graphql_lsp::run_server();
}
