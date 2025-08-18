use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use clap::Parser;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use ts_rs::TS;

#[derive(Parser, Debug)]
#[command(about = "Generate TypeScript bindings for the Codex protocol")]
struct Args {
    /// Output directory where .ts files will be written
    #[arg(short = 'o', long = "out", value_name = "DIR")]
    out_dir: PathBuf,

    /// Path to the Prettier executable to format generated files
    #[arg(short = 'p', long = "prettier", value_name = "PRETTIER_BIN")]
    prettier: PathBuf,
}

const HEADER: &str = "// GENERATED CODE! DO NOT MODIFY BY HAND!\n\n";

fn main() -> Result<()> {
    let args = Args::parse();
    ensure_dir(&args.out_dir)?;

    // Generate TS bindings
    let out = args.out_dir.to_string_lossy().into_owned();
    for f in &[
        codex_protocol::mcp_protocol::ClientRequest::export_all_to,
        codex_protocol::mcp_protocol::ApplyPatchApprovalResponse::export_all_to,
        codex_protocol::mcp_protocol::ExecCommandApprovalResponse::export_all_to,
    ] {
        f(out.clone())?;
    }

    // Prepend header to each generated .ts file
    let ts_files = ts_files_in(&args.out_dir)?;
    for file in &ts_files {
        prepend_header_if_missing(file)?;
    }

    // Format with Prettier by passing individual files (no shell globbing)
    if !ts_files.is_empty() {
        let status = Command::new(&args.prettier)
            .arg("--write")
            .args(ts_files.iter().map(|p| p.as_os_str()))
            .status()
            .with_context(|| format!("Failed to invoke Prettier at {}", args.prettier.display()))?;
        if !status.success() {
            return Err(anyhow!("Prettier failed with status {}", status));
        }
    }

    Ok(())
}

fn ensure_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create output directory {}", dir.display()))
}

fn prepend_header_if_missing(path: &Path) -> Result<()> {
    let mut content = String::new();
    {
        let mut f = fs::File::open(path)
            .with_context(|| format!("Failed to open {} for reading", path.display()))?;
        f.read_to_string(&mut content)
            .with_context(|| format!("Failed to read {}", path.display()))?;
    }

    if content.starts_with(HEADER) {
        return Ok(());
    }

    let mut f = fs::File::create(path)
        .with_context(|| format!("Failed to open {} for writing", path.display()))?;
    f.write_all(HEADER.as_bytes())
        .with_context(|| format!("Failed to write header to {}", path.display()))?;
    f.write_all(content.as_bytes())
        .with_context(|| format!("Failed to write content to {}", path.display()))?;
    Ok(())
}

fn ts_files_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("Failed to read dir {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension() == Some(OsStr::new("ts")) {
            files.push(path);
        }
    }
    Ok(files)
}
