use clap::Parser;
use codex_session::run_main;
use codex_session::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run_main(cli).await?;

    Ok(())
}
