use std::path::PathBuf;

pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    // file:// URL → filesystem path
    if let Ok(url) = url::Url::parse(pasted) {
        if url.scheme() == "file" {
            return url.to_file_path().ok();
        }
    }

    // shell-escaped single path → unescaped
    let parts: Vec<String> = shlex::Shlex::new(pasted).collect();
    if parts.len() == 1 {
        return parts.into_iter().next().map(PathBuf::from);
    }

    // simple quoted path fallback
    Some(PathBuf::from(pasted.trim_matches('"')))
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
    fn img_format_label_png_jpeg_unknown() {
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.PNG")), "PNG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.jpg")), "JPEG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.JPEG")), "JPEG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c")), "IMG");
        assert_eq!(get_img_format_label(PathBuf::from("/a/b/c.webp")), "IMG");
    }
}
