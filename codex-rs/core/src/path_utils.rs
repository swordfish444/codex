use std::io;
use std::path::PathBuf;

pub(crate) fn expand_tilde(raw: &str) -> io::Result<PathBuf> {
    if raw.starts_with('~') {
        // `shellexpand::tilde` falls back to returning the input when the home directory
        // cannot be resolved; mirror the previous error semantics in that case.
        let expanded = shellexpand::tilde(raw);
        if expanded.starts_with('~') {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "could not resolve home directory while expanding path",
            ));
        }
        return Ok(PathBuf::from(expanded.as_ref()));
    }

    Ok(PathBuf::from(raw))
}
