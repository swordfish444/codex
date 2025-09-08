use std::env;
use std::path::Path;
use std::path::PathBuf;

use super::errors::UnifiedExecError;

pub(crate) fn parse_command_line(line: &str) -> Result<Vec<String>, UnifiedExecError> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(UnifiedExecError::MissingCommandLine);
    }

    match parse_command_line_impl(trimmed) {
        Some(parts) if !parts.is_empty() => Ok(parts),
        _ => Err(UnifiedExecError::InvalidCommandLine {
            command_line: trimmed.to_string(),
        }),
    }
}

#[cfg(not(windows))]
fn parse_command_line_impl(trimmed: &str) -> Option<Vec<String>> {
    shlex::split(trimmed)
}

#[cfg(windows)]
fn parse_command_line_impl(trimmed: &str) -> Option<Vec<String>> {
    windows_split_command_line(trimmed)
}

#[cfg(windows)]
fn windows_split_command_line(input: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut backslashes = 0usize;

    for ch in input.chars() {
        match ch {
            '\\' => {
                backslashes += 1;
            }
            '"' => {
                current.extend(std::iter::repeat('\\').take(backslashes / 2));
                if backslashes % 2 == 0 {
                    in_quotes = !in_quotes;
                } else {
                    current.push('"');
                }
                backslashes = 0;
            }
            c if c.is_whitespace() && !in_quotes => {
                current.extend(std::iter::repeat('\\').take(backslashes));
                backslashes = 0;
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            other => {
                current.extend(std::iter::repeat('\\').take(backslashes));
                backslashes = 0;
                current.push(other);
            }
        }
    }

    if in_quotes {
        return None;
    }

    current.extend(std::iter::repeat('\\').take(backslashes));
    if !current.is_empty() {
        args.push(current);
    }

    if args.is_empty() { None } else { Some(args) }
}

pub(crate) fn command_from_chunks(chunks: &[String]) -> Result<Vec<String>, UnifiedExecError> {
    match chunks {
        [] => Err(UnifiedExecError::MissingCommandLine),
        [single] => parse_command_line(single),
        _ => Ok(chunks.to_vec()),
    }
}

pub(crate) fn join_input_chunks(chunks: &[String]) -> String {
    match chunks {
        [] => String::new(),
        [single] => single.clone(),
        _ => chunks.concat(),
    }
}

pub(crate) fn resolve_command_path(command: &str) -> Result<String, UnifiedExecError> {
    if command.is_empty() {
        return Err(UnifiedExecError::MissingCommandLine);
    }

    if is_explicit_path(command) {
        return ensure_executable(Path::new(command))
            .then_some(command.to_string())
            .ok_or_else(|| UnifiedExecError::CommandNotFound {
                command: command.to_string(),
            });
    }

    if let Some(resolved) = find_in_path(command) {
        return Ok(resolved.to_string_lossy().to_string());
    }

    Err(UnifiedExecError::CommandNotFound {
        command: command.to_string(),
    })
}

fn is_explicit_path(command: &str) -> bool {
    let path = Path::new(command);
    path.is_absolute() || path.components().count() > 1
}

fn find_in_path(command: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .flat_map(|dir| candidate_paths(dir, command))
        .find(|candidate| ensure_executable(candidate))
}

fn candidate_paths(dir: PathBuf, command: &str) -> Vec<PathBuf> {
    build_platform_candidates(dir.join(command))
}

#[cfg(unix)]
fn build_platform_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    vec![candidate]
}

#[cfg(windows)]
fn build_platform_candidates(candidate: PathBuf) -> Vec<PathBuf> {
    if candidate.extension().is_some() {
        return vec![candidate];
    }

    let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut candidates = Vec::new();
    for ext in pathext.split(';') {
        if ext.is_empty() {
            continue;
        }
        let mut path_with_ext = candidate.clone();
        let new_ext = ext.trim_start_matches('.');
        path_with_ext.set_extension(new_ext);
        candidates.push(path_with_ext);
    }
    if candidates.is_empty() {
        candidates.push(candidate);
    }
    candidates
}

fn ensure_executable(path: &Path) -> bool {
    match path.metadata() {
        Ok(metadata) => metadata.is_file() && is_executable(&metadata),
        Err(_) => false,
    }
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file()
}

#[cfg(all(test, windows))]
mod tests {
    use super::parse_command_line;

    #[test]
    fn parses_windows_command_with_spaces() {
        let out = parse_command_line(r#""C:\Program Files\Git\bin\bash.exe" -i"#).unwrap();
        assert_eq!(
            out,
            vec![
                "C:\\Program Files\\Git\\bin\\bash.exe".to_string(),
                "-i".to_string(),
            ]
        );
    }
}
