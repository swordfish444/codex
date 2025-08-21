use std::path::PathBuf;

pub fn normalize_pasted_path(pasted: &str) -> Option<PathBuf> {
    // file:// URL → filesystem path
    if let Ok(url) = url::Url::parse(pasted) {
        if url.scheme() == "file" {
            return url.to_file_path().ok();
        }
    }

    // shell-escaped single path → unescaped
    if let Ok(mut parts) = shell_words::split(pasted) {
        if parts.len() == 1 {
            return Some(PathBuf::from(parts.remove(0)));
        }
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
