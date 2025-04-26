//! Library entry-point re-exporting the CLI so the binary can stay tiny.

pub mod cli;
mod spawn;
mod store;

pub use cli::Cli;

/// Binary entry – the bin crate’s `main.rs` calls into this for testability.
pub async fn run_main(cli: Cli) -> anyhow::Result<()> {
    cli.dispatch().await
}

