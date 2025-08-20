use std::path::PathBuf;

#[derive(Debug)]
pub enum PasteImageError {
    ClipboardUnavailable(String),
    NoImage(String),
    EncodeFailed(String),
    IoError(String),
}

impl std::fmt::Display for PasteImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasteImageError::ClipboardUnavailable(msg) => write!(f, "clipboard unavailable: {msg}"),
            PasteImageError::NoImage(msg) => write!(f, "no image on clipboard: {msg}"),
            PasteImageError::EncodeFailed(msg) => write!(f, "could not encode image: {msg}"),
            PasteImageError::IoError(msg) => write!(f, "io error: {msg}"),
        }
    }
}
impl std::error::Error for PasteImageError {}

#[derive(Debug, Clone)]
pub struct PastedImageInfo {
    pub width: u32,
    pub height: u32,
    pub encoded_format_label: &'static str, // Always PNG for now.
}

/// Capture image from system clipboard, encode to PNG, and return bytes + info.
pub fn paste_image_as_png() -> Result<(Vec<u8>, PastedImageInfo), PasteImageError> {
    tracing::debug!("attempting clipboard image read");
    let mut cb = arboard::Clipboard::new()
        .map_err(|e| PasteImageError::ClipboardUnavailable(e.to_string()))?;
    let img = cb
        .get_image()
        .map_err(|e| PasteImageError::NoImage(e.to_string()))?;
    let w = img.width as u32;
    let h = img.height as u32;

    let mut png: Vec<u8> = Vec::new();
    let Some(rgba_img) = image::RgbaImage::from_raw(w, h, img.bytes.into_owned()) else {
        return Err(PasteImageError::EncodeFailed("invalid RGBA buffer".into()));
    };
    let dyn_img = image::DynamicImage::ImageRgba8(rgba_img);
    tracing::debug!("clipboard image decoded RGBA {}x{}", w, h);
    {
        let mut cursor = std::io::Cursor::new(&mut png);
        dyn_img
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| PasteImageError::EncodeFailed(e.to_string()))?;
    }

    tracing::debug!("clipboard image encoded to PNG ({} bytes)", png.len());
    Ok((
        png,
        PastedImageInfo {
            width: w,
            height: h,
            encoded_format_label: "PNG",
        },
    ))
}

/// Convenience: write to a temp file and return its path + info.
pub fn paste_image_to_temp_png() -> Result<(PathBuf, PastedImageInfo), PasteImageError> {
    let (png, info) = paste_image_as_png()?;
    let mut path = std::env::temp_dir();
    let fname = format!("clipboard-{}x{}.png", info.width, info.height);
    path.push(fname);
    std::fs::write(&path, &png).map_err(|e| PasteImageError::IoError(e.to_string()))?;
    Ok((path, info))
}

/// macOS-specific: Try extracting image file paths from the system pasteboard
/// when the user copied a file in Finder. Prefer attaching the actual file
/// instead of the small icon bitmap that may also be present on the clipboard.
#[cfg(target_os = "macos")]
pub fn image_file_from_clipboard_macos() -> Option<PathBuf> {
    fn run_osascript(lines: &[&str]) -> Option<String> {
        use std::process::Command;
        let output = Command::new("osascript")
            .args(lines.iter().flat_map(|l| ["-e", *l]))
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    }

    // 1) Try to read a list of aliases (multiple files)
    if let Some(out) = run_osascript(&[
        "try",
        "set theFiles to the clipboard as alias list",
        "set out to \"\"",
        "repeat with f in theFiles",
        "set out to out & POSIX path of f & \"\n\"",
        "end repeat",
        "out",
        "end try",
    ]) {
        for line in out.lines() {
            let p = std::path::PathBuf::from(line.trim());
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let ext = ext.to_ascii_lowercase();
                    if matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
                        return Some(p);
                    }
                }
            }
        }
    }

    // 2) Fallback: single alias
    if let Some(out) = run_osascript(&["try", "POSIX path of (the clipboard as alias)", "end try"])
    {
        let p = std::path::PathBuf::from(out.trim());
        if p.is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                let ext = ext.to_ascii_lowercase();
                if matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
                    return Some(p);
                }
            }
        }
    }

    None
}
