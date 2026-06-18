mod alembic;
mod cli;
mod config;
mod features;
mod model;
mod parsing;
mod server;
mod state;
mod util;

use clap::{Parser, Subcommand};
use server::Backend;
use tower_lsp_server::{LspService, Server};

use cli::check::{CheckArgs, run_check};
use cli::schema::{SchemaArgs, run_schema};
use cli::stats::{StatsArgs, run_stats};

#[derive(Parser)]
#[command(
    name = "sqlalchemy-lsp",
    version = concat!(env!("CARGO_PKG_VERSION"), " (build: ", env!("BUILD_TIMESTAMP"), ")"),
    about = "Language server for SQLAlchemy ORM intelligence"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the language server over stdio
    Lsp,
    /// Run headless diagnostics (CI linter)
    Check(CheckArgs),
    /// Print workspace ER schema
    Schema(SchemaArgs),
    /// Print workspace model statistics
    Stats(StatsArgs),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Lsp) {
        Command::Lsp => run_lsp().await,
        Command::Check(args) => std::process::exit(run_check(args)),
        Command::Schema(args) => std::process::exit(run_schema(args)),
        Command::Stats(args) => std::process::exit(run_stats(args)),
    }
}

async fn run_lsp() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
