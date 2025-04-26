//! Library entry-point re-exporting the CLI so the binary can stay tiny.

//! Manage background `codex-exec` agents.
//!
//! This library is thin: it only re-exports the clap CLI and helpers so
//! the binary can stay small and unit tests can call into pure Rust APIs.

pub mod cli; // public so main.rs can access it.
mod spawn; // process creation helpers
pub mod store; // on-disk bookkeeping (public for tests)
pub mod meta; // richer on-disk metadata envelope
pub mod build; // build-time information helpers

pub use cli::Cli;

/// Entry used by the bin crate.
pub async fn run_main(cli: Cli) -> anyhow::Result<()> {
    cli.dispatch().await
}
