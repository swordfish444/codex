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

// Convert a file:// URL into a local path.
// Rules:
// - Accept only empty host or "localhost"; reject remote hosts.
// - On Unix, require absolute paths (leading '/').
// - On Windows, support forms like file:///C:/path and file://localhost/C:/path.
pub(crate) fn file_url_to_path(s: &str) -> Option<std::path::PathBuf> {
    let Some(mut rest) = s.strip_prefix("file://") else {
        return None;
    };

    // Handle optional host (e.g., file://localhost/...). Only allow empty or localhost.
    if let Some(after_host) = rest.strip_prefix("localhost") {
        rest = after_host;
    } else if rest.starts_with('/') {
        // empty host, keep as-is
    } else {
        // Non-local host is not supported – reject.
        return None;
    }

    // Percent-decode the path portion
    let decoded = percent_decode_to_string(rest)?;

    #[cfg(windows)]
    {
        // On Windows, URLs often look like file:///C:/path or file://localhost/C:/path
        // If the decoded path starts with '/<drive>:/', strip the leading slash.
        let path = if decoded.len() >= 4
            && decoded.as_bytes()[0] == b'/'
            && decoded.as_bytes()[1].is_ascii_alphabetic()
            && decoded.as_bytes()[2] == b':'
            && decoded.as_bytes()[3] == b'/'
        {
            decoded[1..].to_string()
        } else {
            decoded
        };
        return Some(std::path::PathBuf::from(path));
    }

    #[cfg(not(windows))]
    {
        // On Unix, require absolute path.
        if !decoded.starts_with('/') {
            return None;
        }
        Some(std::path::PathBuf::from(decoded))
    }
}

// Unescape simple bash-style backslash escapes (e.g., spaces, parens).
pub(crate) fn unescape_backslashes(s: &str) -> String {
    // On Windows, do not unescape backslashes; they are core to paths like C:\Users.
    #[cfg(windows)]
    {
        return s.to_string();
    }

    // On Unix, unescape common shell-escaped characters (e.g., spaces, parens, quotes).
    #[cfg(not(windows))]
    {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(n) = chars.next() {
                    // Only unescape a known set of characters typically escaped in shells.
                    let should_unescape = matches!(
                        n,
                        ' ' | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '\\'
                            | '$'
                            | '&'
                            | '!'
                            | '#'
                            | ';'
                            | ':'
                            | '@'
                            | '='
                            | '+'
                            | ','
                            | '~'
                            | '|'
                            | '<'
                            | '>'
                            | '?'
                            | '*'
                    );
                    if should_unescape {
                        out.push(n);
                    } else {
                        out.push('\\');
                        out.push(n);
                    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_basic() {
        assert_eq!(percent_decode_to_string("/a%20b").as_deref(), Some("/a b"));
    }

    #[test]
    #[cfg(not(windows))]
    fn file_url_localhost_unix() {
        assert_eq!(
            file_url_to_path("file://localhost//tmp/foo").unwrap(),
            std::path::PathBuf::from("/tmp/foo")
        );
        assert_eq!(
            file_url_to_path("file:////tmp/foo").unwrap(),
            std::path::PathBuf::from("/tmp/foo")
        );
        assert!(file_url_to_path("file://host/tmp/foo").is_none());
        assert!(file_url_to_path("file://localhosttmp/foo").is_none());
    }

    #[test]
    #[cfg(windows)]
    fn file_url_windows_drive() {
        assert_eq!(
            file_url_to_path("file:///C:/Users/test").unwrap(),
            std::path::PathBuf::from("C:/Users/test")
        );
        assert_eq!(
            file_url_to_path("file://localhost/C:/Users/test").unwrap(),
            std::path::PathBuf::from("C:/Users/test")
        );
        assert!(file_url_to_path("file://host/C:/Users/test").is_none());
    }

    #[test]
    #[cfg(not(windows))]
    fn unescape_backslashes_unix() {
        assert_eq!(unescape_backslashes("My\\ File(1).png"), "My File(1).png");
        // Leave unknown escapes intact
        assert_eq!(unescape_backslashes("abc\\z"), "abc\\z");
    }

    #[test]
    #[cfg(windows)]
    fn unescape_backslashes_windows_noop() {
        assert_eq!(unescape_backslashes("C:\\Users\\test"), "C:\\Users\\test");
    }
}
