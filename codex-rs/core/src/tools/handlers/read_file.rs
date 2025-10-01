use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use codex_utils_string::take_bytes_at_char_boundary;
use serde::Deserialize;
use tokio::fs::File;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;

use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct ReadFileHandler;

const MAX_LINE_LENGTH: usize = 200;

fn default_offset() -> usize {
    1
}

fn default_limit() -> usize {
    2000
}

#[derive(Deserialize)]
struct ReadFileArgs {
    file_path: String,
    #[serde(default = "default_offset")]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[async_trait]
impl ToolHandler for ReadFileHandler {
    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(
        &self,
        invocation: ToolInvocation<'_>,
    ) -> Result<ToolOutput, FunctionCallError> {
        let ToolInvocation { payload, .. } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "read_file handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: ReadFileArgs = serde_json::from_str(&arguments).map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to parse function arguments: {err:?}"
            ))
        })?;

        let ReadFileArgs {
            file_path,
            offset,
            limit,
        } = args;

        if offset == 0 {
            return Err(FunctionCallError::RespondToModel(
                "offset must be a 1-indexed line number".to_string(),
            ));
        }

        if limit == 0 {
            return Err(FunctionCallError::RespondToModel(
                "limit must be greater than zero".to_string(),
            ));
        }

        let path = PathBuf::from(&file_path);
        if !path.is_absolute() {
            return Err(FunctionCallError::RespondToModel(
                "file_path must be an absolute path".to_string(),
            ));
        }

        let collected = read_file_slice(&path, offset, limit).await?;
        Ok(ToolOutput::Function {
            content: collected.join("\n"),
            success: Some(true),
        })
    }
}

async fn read_file_slice(
    path: &Path,
    offset: usize,
    limit: usize,
) -> Result<Vec<String>, FunctionCallError> {
    let file = File::open(path)
        .await
        .map_err(|err| FunctionCallError::RespondToModel(format!("failed to read file: {err}")))?;

    let mut reader = BufReader::new(file).lines();
    let mut collected = Vec::new();
    let mut seen = 0usize;

    while let Some(line) = reader
        .next_line()
        .await
        .map_err(|err| FunctionCallError::RespondToModel(format!("failed to read file: {err}")))?
    {
        seen += 1;

        if seen < offset {
            continue;
        }

        if collected.len() == limit {
            break;
        }

        let formatted = if line.len() > MAX_LINE_LENGTH {
            take_bytes_at_char_boundary(&line, MAX_LINE_LENGTH).to_string()
        } else {
            line
        };

        collected.push(format!("L{seen}: {formatted}"));

        if collected.len() == limit {
            break;
        }
    }

    if seen < offset {
        return Err(FunctionCallError::RespondToModel(
            "offset exceeds file length".to_string(),
        ));
    }

    Ok(collected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn reads_requested_range() {
        let mut temp = NamedTempFile::new().expect("create temp file");
        use std::io::Write as _;
        writeln!(temp, "alpha").unwrap();
        writeln!(temp, "beta").unwrap();
        writeln!(temp, "gamma").unwrap();

        let lines = read_file_slice(temp.path(), 2, 2)
            .await
            .expect("read slice");
        assert_eq!(lines, vec!["L2: beta".to_string(), "L3: gamma".to_string()]);
    }

    #[tokio::test]
    async fn errors_when_offset_exceeds_length() {
        let mut temp = NamedTempFile::new().expect("create temp file");
        use std::io::Write as _;
        writeln!(temp, "only").unwrap();

        let err = read_file_slice(temp.path(), 3, 1)
            .await
            .expect_err("offset exceeds length");
        assert_eq!(
            err,
            FunctionCallError::RespondToModel("offset exceeds file length".to_string())
        );
    }
}
