use codex_arg0::arg0_dispatch_or_else;
use codex_common::CliConfigOverrides;
use codex_mcp_server::run_main;

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|sandbox_executables| async move {
        run_main(sandbox_executables, CliConfigOverrides::default()).await?;
        Ok(())
    })
}
