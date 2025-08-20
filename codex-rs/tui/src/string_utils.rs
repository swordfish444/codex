// String and path parsing helpers used across the TUI.

// Naive percent-decoding for file:// URL paths; returns None on invalid UTF-8.
pub(crate) fn percent_decode_to_string(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1];
            let h2 = bytes[i + 2];
            let hex = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            if let (Some(x), Some(y)) = (hex(h1), hex(h2)) {
                out.push(x * 16 + y);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

// Convert a file:// URL into a local path (macOS/Unix only, UTF-8).
pub(crate) fn file_url_to_path(s: &str) -> Option<std::path::PathBuf> {
    if let Some(rest) = s.strip_prefix("file://") {
        // Strip optional host like file://localhost/...
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let decoded = percent_decode_to_string(rest)?;
        let p = std::path::PathBuf::from(decoded);
        return Some(p);
    }
    None
}

// Unescape simple bash-style backslash escapes (e.g., spaces, parens).
pub(crate) fn unescape_backslashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            } else {
                // Trailing backslash; keep it.
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}
