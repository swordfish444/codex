use std::path::PathBuf;

use clap::Parser;
use codex_common::{CliConfigOverrides, SandboxPermissionOption};
use codex_cli::debug_sandbox::create_sandbox_policy;
use codex_core::config::{Config, ConfigOverrides};
use codex_core::protocol::SandboxPolicy;

/// Inspect the sandbox and container environment (mounts, permissions, network)
#[derive(Debug, Parser)]
pub struct InspectEnvArgs {
    /// Convenience alias for low-friction sandboxed automatic execution (network-disabled sandbox that can write to cwd and TMPDIR)
    #[arg(long = "full-auto", default_value_t = false)]
    pub full_auto: bool,

    /// Sandbox permission overrides (network, mounts)
    #[clap(flatten)]
    pub sandbox: SandboxPermissionOption,

    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,
}

/// Run the inspect-env command.
pub async fn run_inspect_env(
    args: InspectEnvArgs,
    codex_linux_sandbox_exe: Option<PathBuf>,
) -> anyhow::Result<()> {
    // Build sandbox policy from CLI flags.
    let sandbox_policy = create_sandbox_policy(args.full_auto, args.sandbox);
    // Load configuration to include any -c overrides and sandbox policy.
    let config = Config::load_with_cli_overrides(
        args.config_overrides.parse_overrides().map_err(anyhow::Error::msg)?,
        ConfigOverrides {
            sandbox_policy: Some(sandbox_policy.clone()),
            codex_linux_sandbox_exe,
            ..Default::default()
        },
    )?;
    let policy = &config.sandbox_policy;
    let cwd = &config.cwd;

    // Compute mount entries: root and writable roots.
    let mut mounts = Vec::new();
    if policy.has_full_disk_write_access() {
        mounts.push(("/".to_string(), "rw".to_string()));
    } else if policy.has_full_disk_read_access() {
        mounts.push(("/".to_string(), "ro".to_string()));
    }
    let writable_roots = policy.get_writable_roots_with_cwd(cwd);
    for root in writable_roots.iter() {
        let path = root.display().to_string();
        if path != "/" {
            mounts.push((path, "rw".to_string()));
        }
    }

    // Determine column width for PATH.
    let width = mounts.iter().map(|(p, _)| p.len()).max().unwrap_or(0).max(4);

    // Header.
    println!("Sandbox & Container Environment\n");

    // Mounts.
    println!("Mounts:");
    println!("  {:<width$}  {}", "PATH", "MODE", width = width);
    println!("  {:-<width$}  {:-<4}", "", "", width = width);
    for (path, mode) in &mounts {
        println!("  {:<width$}  {}", path, mode, width = width);
    }
    println!();

    // Permissions.
    println!("Permissions:");
    for perm in policy.permissions() {
        println!("  - {:?}", perm);
    }
    println!();

    // Network status.
    let net = if policy.has_full_network_access() { "enabled" } else { "disabled" };
    println!("Network: {}", net);
    println!();

    // Summary.
    println!("Summary:");
    println!("  Mount count: {}", mounts.len());
    println!("  Writable roots: {}", writable_roots.len());

    Ok(())
}
