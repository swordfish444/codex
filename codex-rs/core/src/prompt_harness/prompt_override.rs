use std::fs::File;
use std::io::Read;
use std::io::{self};
use std::path::Path;

const PROMPT_OVERRIDE_MAX_BYTES: u64 = 8 * 1024;

pub fn load_system_prompt_override(path: &Path) -> io::Result<String> {
    let metadata = path.metadata().map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "failed to read system prompt override metadata {}: {err}",
                path.display()
            ),
        )
    })?;

    if metadata.len() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("system prompt override file is empty: {}", path.display()),
        ));
    }

    if metadata.len() > PROMPT_OVERRIDE_MAX_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "system prompt override exceeds limit ({} bytes): {}",
                PROMPT_OVERRIDE_MAX_BYTES,
                path.display()
            ),
        ));
    }

    let mut file = File::open(path).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "failed to open system prompt override {}: {err}",
                path.display()
            ),
        )
    })?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(|err| {
        io::Error::new(
            err.kind(),
            format!(
                "failed to read system prompt override {}: {err}",
                path.display()
            ),
        )
    })?;

    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "system prompt override only contained whitespace: {}",
                path.display()
            ),
        ));
    }

    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn rejects_missing_file() {
        let path = std::path::PathBuf::from("/no/such/file");
        let err = load_system_prompt_override(&path).expect_err("expected error");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn rejects_empty_file() {
        let file = NamedTempFile::new().expect("create temp");
        std::fs::write(file.path(), "   \n \n").expect("write temp");
        let err = load_system_prompt_override(file.path()).expect_err("expected error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("whitespace"));
    }

    #[test]
    fn rejects_large_file() {
        let file = NamedTempFile::new().expect("create temp");
        let large = vec![b'x'; (PROMPT_OVERRIDE_MAX_BYTES + 1) as usize];
        std::fs::write(file.path(), large).expect("write temp");
        let err = load_system_prompt_override(file.path()).expect_err("expected error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("exceeds limit"));
    }

    #[test]
    fn trims_and_returns_contents() {
        let file = NamedTempFile::new().expect("create temp");
        std::fs::write(file.path(), "\n  hello world  \n").expect("write temp");
        let prompt = load_system_prompt_override(file.path()).expect("load prompt");
        assert_eq!(prompt, "hello world");
    }
}
