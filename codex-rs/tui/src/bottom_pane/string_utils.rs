use std::path::PathBuf;

pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    let pasted = pasted.trim();
    // file:// URL → filesystem path
    if let Ok(url) = url::Url::parse(pasted) {
        if url.scheme() == "file" {
            return url.to_file_path().ok();
        }
    }

    // Windows paths (unquoted) → bypass POSIX shlex to preserve backslashes
    // - Drive letter paths like C:\Users\Alice\img.png
    // - UNC paths like \\server\share\img.png
    let looks_like_windows_drive_path = pasted.len() >= 3
        && pasted.as_bytes()[1] == b':'
        && (pasted.as_bytes()[2] == b'/' || pasted.as_bytes()[2] == b'\\')
        && pasted.as_bytes()[0].is_ascii_alphabetic();
    let looks_like_unc = pasted.starts_with("\\\\");
    let is_quoted = (pasted.starts_with('"') && pasted.ends_with('"'))
        || (pasted.starts_with('\'') && pasted.ends_with('\''));
    if (looks_like_windows_drive_path || looks_like_unc) && !is_quoted {
        return Some(PathBuf::from(pasted));
    }

    // shell-escaped single path → unescaped
    let parts: Vec<String> = shlex::Shlex::new(pasted).collect();
    if parts.len() == 1 {
        return parts.into_iter().next().map(PathBuf::from);
    }

    None
}

pub fn get_img_format_label(path: PathBuf) -> String {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
    {
        Some(ext) if ext == "png" => "PNG",
        Some(ext) if ext == "jpg" || ext == "jpeg" => "JPEG",
        _ => "IMG",
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_file_url() {
        let input = "file:///tmp/example.png";
        let result = normalize_pasted_path(input).expect("should parse file URL");
        assert_eq!(result, PathBuf::from("/tmp/example.png"));
    }

    #[test]
    fn normalize_shell_escaped_single_path() {
        let input = "/home/user/My\\ File.png";
        let result = normalize_pasted_path(input).expect("should unescape shell-escaped path");
        assert_eq!(result, PathBuf::from("/home/user/My File.png"));
    }

    #[test]
    fn normalize_simple_quoted_path_fallback() {
        let input = "\"/home/user/My File.png\"";
        let result = normalize_pasted_path(input).expect("should trim simple quotes");
        assert_eq!(result, PathBuf::from("/home/user/My File.png"));
    }

    #[test]
    fn normalize_single_quoted_unix_path() {
        let input = "'/home/user/My File.png'";
        let result = normalize_pasted_path(input).expect("should trim single quotes via shlex");
        assert_eq!(result, PathBuf::from("/home/user/My File.png"));
    }

    #[test]
    fn normalize_single_quoted_windows_path() {
        let input = r"'C:\Users\Alice\My File.jpeg'";
        let result =
            normalize_pasted_path(input).expect("should trim single quotes on windows path");
        assert_eq!(result, PathBuf::from(r"C:\Users\Alice\My File.jpeg"));
    }

    #[test]
    fn normalize_multiple_tokens_returns_none() {
        // Two tokens after shell splitting → not a single path
        let input = "/home/user/a\\ b.png /home/user/c.png";
        let result = normalize_pasted_path(input);
        assert!(result.is_none());
    }

    #[test]
    fn img_format_label_png_jpeg_unknown() {
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.PNG")), "PNG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.jpg")), "JPEG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.JPEG")), "JPEG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c")), "IMG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.webp")), "IMG");
    }

    #[test]
    fn img_format_label_with_windows_style_paths() {
        assert_eq!(get_img_format_label(PathBuf::from(r"C:\a\b\c.PNG")), "PNG");
        assert_eq!(
            get_img_format_label(PathBuf::from(r"C:\a\b\c.jpeg")),
            "JPEG"
        );
        assert_eq!(get_img_format_label(PathBuf::from(r"C:\a\b\noext")), "IMG");
    }

    #[test]
    fn normalize_unquoted_windows_path() {
        let input = r"C:\Users\Alice\img.png";
        let result = normalize_pasted_path(input).expect("should accept unquoted windows path");
        assert_eq!(result, PathBuf::from(r"C:\Users\Alice\img.png"));
    }
}
