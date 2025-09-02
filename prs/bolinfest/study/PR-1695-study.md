**DOs**

- **Keep app.rs lean:** Drive the event loop and delegate event-specific handling to modules.
  ```
  // app.rs
  match event {
      TuiEvent::AttachImage { path, width, height, format_label } => {
          self.chat_widget.attach_image(path, width, height, format_label);
      }
      TuiEvent::Key(key) => self.chat_widget.handle_key_event(key),
      TuiEvent::Paste(s) => self.chat_widget.handle_paste(s),
      TuiEvent::Draw => self.redraw()?,
  }
  ```

- **Generate rich events in the TUI layer:** Map platform input to semantic TuiEvents before app.rs sees them.
  ```
  // tui.rs
  match crossterm_event {
      Event::Key(k) if is_ctrl_or_cmd_v(&k) => {
          if let Ok((path, info)) = paste_image_to_temp_png() {
              yield TuiEvent::AttachImage { path, width: info.width, height: info.height, format_label: info.encoded_format.label() };
          } else {
              yield TuiEvent::Key(k);
          }
      }
      Event::Paste(s) => yield TuiEvent::Paste(s),
      Event::Key(k) => yield TuiEvent::Key(k),
      Event::Resize(_, _) => yield TuiEvent::Draw,
      _ => {}
  }
  ```

- **Handle cross-platform shortcuts:** Don’t assume macOS Cmd maps to Control; accept Control, Super, or Meta for V.
  ```
  use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

  fn is_ctrl_or_cmd_v(k: &KeyEvent) -> bool {
      k.kind == KeyEventKind::Press
          && matches!(k.code, KeyCode::Char('v'))
          && (k.modifiers.contains(KeyModifiers::CONTROL)
              || k.modifiers.contains(KeyModifiers::SUPER)
              || k.modifiers.contains(KeyModifiers::META))
  }
  ```

- **Model image formats with an enum, not strings:** Add a label() to present user-friendly text.
  ```
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  enum EncodedImageFormat { Png }

  impl EncodedImageFormat {
      fn label(self) -> &'static str {
          match self { EncodedImageFormat::Png => "PNG" }
      }
  }
  ```

- **Write clipboard images via tempfile and persist safely:**
  ```
  use tempfile::NamedTempFile;

  let (png_bytes, info) = paste_image_as_png()?;
  let tmp = NamedTempFile::new()?;              // unique, race-free
  std::fs::write(tmp.path(), &png_bytes)?;
  let (_file, path) = tmp.keep()?;              // persist and get PathBuf
  ```

- **Prefer small structs over tuples for clarity:** Make stored state self-documenting.
  ```
  #[derive(Clone, Debug, PartialEq)]
  struct AttachedImage {
      placeholder: String,
      path: PathBuf,
  }
  ```

- **Make image placeholders atomic UI elements and keep mappings in sync:**
  ```
  // Insert atomic element so a single Backspace removes it.
  self.textarea.insert_element(&placeholder);
  self.attached_images.push(AttachedImage { placeholder, path });

  // On edit, keep only as many AttachedImage entries as visible placeholders.
  let visible = text_after.matches(&img.placeholder).count();
  ```

- **Strip placeholders before submit; drain images separately:**
  ```
  // composer.rs (on Enter)
  for img in &self.attached_images {
      text = text.replace(&img.placeholder, "");
  }
  text = text.trim().to_string();

  // chatwidget.rs
  let images = self.bottom_pane.take_recent_submission_images();
  self.submit_user_message(UserMessage { text, image_paths: images });
  ```

- **Inline variables in logs and formats:** Use modern capture in tracing/format! calls.
  ```
  tracing::info!("attach_image path={path:?} width={width} height={height} format={format_label}");
  let msg = format!("added {count} images from {source}");
  ```

- **Use local imports and concise types; move imports to the module top:**
  ```
  use std::path::PathBuf;           // at top of file
  // ...
  fn attach_image(&mut self, path: PathBuf, width: u32, height: u32, format_label: &str) { /* ... */ }
  ```

- **Test user-visible behaviors thoroughly:** Attach + submit, deletion at both ends, and duplicates.
  ```
  #[test]
  fn deleting_one_of_duplicate_image_placeholders_removes_matching_entry() {
      composer.attach_image(p1.clone(), 10, 5, "PNG");
      composer.handle_paste(" ".into());
      composer.attach_image(p2.clone(), 10, 5, "PNG");
      composer.textarea.set_cursor(end_of_first_placeholder);
      composer.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
      assert_eq!(1, composer.textarea.text().matches(&ph).count());
      assert_eq!(vec![p2], composer.take_recent_submission_images());
  }
  ```

- **Treat @file-selected images as images (with fallback):**
  ```
  if is_image_path(&sel_path) {
      if let Ok((w, h)) = image::image_dimensions(&sel_path) {
          self.attach_image(PathBuf::from(&sel_path), w, h, "PNG"); // or "JPEG"
          self.textarea.insert_str(" ");
      } else {
          self.insert_selected_path(&sel_path); // fallback
      }
  } else {
      self.insert_selected_path(&sel_path);
  }
  ```


**DON’Ts**

- **Don’t put event logic in app.rs:** Keep it for orchestration; push specifics into TUI/chat modules.
  ```
  // ❌ Heavy logic in app.rs
  if is_ctrl_or_cmd_v(&key) { do_clipboard_io_here(); }

  // ✅ Delegate
  TuiEvent::AttachImage { .. } => self.chat_widget.attach_image(...)
  ```

- **Don’t assume Cmd==Control on macOS:** Accept SUPER/META too; fall back gracefully.
  ```
  // ❌ Only CONTROL
  k.modifiers.contains(KeyModifiers::CONTROL)

  // ✅ Cross-platform
  k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META)
  ```

- **Don’t write temp files with ad-hoc names in temp_dir:** Avoid collisions and TOCTOU races.
  ```
  // ❌
  let mut p = std::env::temp_dir(); p.push("clipboard.png"); std::fs::write(p, &bytes)?;

  // ✅
  let tmp = tempfile::NamedTempFile::new()?;
  ```

- **Don’t use ad-hoc strings for image format:** Prefer a typed enum.
  ```
  // ❌
  struct PastedImageInfo { encoded_format_label: &'static str }

  // ✅
  struct PastedImageInfo { encoded_format: EncodedImageFormat }
  ```

- **Don’t return meaningless booleans or keep dead code:** Remove unused return values and placeholders.
  ```
  // ❌
  pub fn attach_image(&mut self, path: PathBuf, w: u32, h: u32, fmt: &str) -> bool { /* always true */ }

  // ✅
  pub fn attach_image(&mut self, path: PathBuf, w: u32, h: u32, fmt: &str) { /* ... */ }
  ```

- **Don’t fully qualify common std types everywhere or import inside tests:** Keep files tidy and readable.
  ```
  // ❌
  fn f(p: std::path::PathBuf) {}

  // ✅
  use std::path::PathBuf;
  fn f(p: PathBuf) {}
  ```

- **Don’t let placeholders and image state drift:** Always update mappings when text changes.
  ```
  // ❌ Forgetting to drop mapping when placeholder deleted.

  // ✅ Remove mapping when matching placeholder instance is removed.
  self.attached_images.remove(idx);
  ```

- **Don’t log with positional/format-args noise:** Use inline captures for clarity and consistency.
  ```
  // ❌
  tracing::info!("path: {:?}, width: {}, height: {}", path, width, height);

  // ✅
  tracing::info!("path={path:?} width={width} height={height}");
  ```