use exec_command_mcp::run_main;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run_main().await?;
    Ok(())
}
