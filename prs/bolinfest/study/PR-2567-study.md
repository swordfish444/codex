**DOs**

- **Normalize pasted paths robustly:** Support `file://` URLs, Windows drive/UNC paths, and single shell-escaped paths.
```rust
pub fn normalize_pasted_path(pasted: &str) -> Option<std::path::PathBuf> {
    let pasted = pasted.trim();

    if let Ok(url) = url::Url::parse(pasted)
        && url.scheme() == "file"
    {
        return url.to_file_path().ok();
    }

    let looks_like_windows = {
        let drive = pasted.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
            && pasted.get(1..2) == Some(":")
            && matches!(pasted.get(2..3), Some("\\") | Some("/"));
        let unc = pasted.starts_with("\\\\");
        drive || unc
    };
    if looks_like_windows {
        return Some(pasted.into());
    }

    let parts: Vec<String> = shlex::Shlex::new(pasted).collect();
    (parts.len() == 1).then(|| parts.into_iter().next().unwrap().into())
}
```

- **Attach images from file paths and keep placeholders in submitted text:** Validate dimensions, infer label, attach; do not strip placeholders on submit.
```rust
pub fn handle_paste_image_path(&mut self, pasted: String) -> bool {
    let Some(path) = normalize_pasted_path(&pasted) else { return false; };
    match image::image_dimensions(&path) {
        Ok((w, h)) => {
            let label = pasted_image_format(&path).label();
            self.attach_image(path, w, h, label);
            true
        }
        Err(_) => false,
    }
}

// In paste handler:
if self.handle_paste_image_path(pasted.clone()) {
    self.textarea.insert_str(" "); // keep placeholder in text
}

// In test:
match result {
    InputResult::Submitted(text) => assert_eq!(text, "[image 10x5 PNG]"),
    _ => panic!("expected Submitted"),
}
```

- **Use a typed enum for image formats with a display label:** Avoid stringly-typed formats.
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedImageFormat { Png, Jpeg, Other }

impl EncodedImageFormat {
    pub fn label(self) -> &'static str {
        match self { Self::Png => "PNG", Self::Jpeg => "JPEG", Self::Other => "IMG" }
    }
}

pub fn pasted_image_format(p: &std::path::Path) -> EncodedImageFormat {
    match p.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("png") => EncodedImageFormat::Png,
        Some("jpg") | Some("jpeg") => EncodedImageFormat::Jpeg,
        _ => EncodedImageFormat::Other,
    }
}
```

- **Write clean, self-cleaning tests:** Use `tempfile::tempdir` and move imports to the top of `mod tests`.
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use tempfile::tempdir;

    #[test]
    fn pasting_filepath_attaches_image() {
        let tmp = tempdir().expect("create TempDir");
        let img_path = tmp.path().join("img.png");
        ImageBuffer::<Rgba<u8>, _>::from_fn(3, 2, |_x, _y| Rgba([1,2,3,255]))
            .save(&img_path)
            .unwrap();

        let needs_redraw = composer.handle_paste(img_path.to_string_lossy().to_string());
        assert!(needs_redraw);
        assert!(composer.textarea.text().starts_with("[image 3x2 PNG] "));
    }
}
```

- **Keep modules purpose-specific (avoid dumping grounds) and format consistently:** Prefer descriptive module names and run formatters.
```rust
// Good: colocate path logic with clipboard handling or use specific modules
// file: tui/src/clipboard_paste.rs           or tui/src/pasted_paths.rs

// Before committing:
$ just fmt
$ just fix -p codex-tui
```

- **Declare necessary dependencies when introducing new APIs:** Add `url` when using `url::Url`.
```toml
# codex-rs/tui/Cargo.toml
[dependencies]
url = "2"
```

**DON’Ts**

- **Don’t create generic “utils.rs” dumping grounds:** Use focused names like `clipboard_paste.rs` or `pasted_paths.rs`.
```rust
// Avoid
// tui/src/string_utils.rs  // collects unrelated helpers over time

// Prefer
// tui/src/pasted_paths.rs  // narrowly focused on pasted path handling
```

- **Don’t manually clean temp files in tests:** Avoid `std::env::temp_dir()` + manual `remove_file`.
```rust
// Avoid
let path = std::env::temp_dir().join("leaky.png");
// ... write file ...
let _ = std::fs::remove_file(&path);

// Prefer
let tmp = tempfile::tempdir()?;
let path = tmp.path().join("scoped.png"); // auto-clean on drop
```

- **Don’t strip image placeholders on submit:** They should remain in the submitted text.
```rust
// Avoid
text = text.replace(&img.placeholder, ""); // removes "[image WxH ...]"

// Prefer
// leave placeholders intact; they represent attached content in text
```

- **Don’t represent image formats as plain strings:** Use a typed enum with a `label()` for display.
```rust
// Avoid
let fmt = "png".to_string();

// Prefer
let fmt = pasted_image_format(path).label();
```

- **Don’t parse Windows paths with POSIX-only assumptions:** Detect drive/UNC forms and bypass POSIX escaping rules.
```rust
// Avoid
let parts: Vec<String> = shlex::Shlex::new(r"C:\Users\Me\img.png").collect(); // mis-parses

// Prefer
if looks_like_windows_path { return Some(PathBuf::from(pasted)); }
```

- **Don’t skip formatting and linting:** Ensure consistent style before landing.
```bash
just fmt
just fix -p codex-tui
cargo test -p codex-tui
```