use std::io;
use std::path::Path;
use std::str::FromStr;

use toml_edit::DocumentMut as Document;
use toml_edit::Item;
use toml_edit::Table;
use toml_edit::Value as TomlValueEdit;

use super::DEFAULT_BASE_URL;

/// Read the list of models recorded under [model_providers.ollama].models.
pub fn read_ollama_models_list(config_path: &Path) -> Vec<String> {
    match std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(toml::Value::Table(root)) => root
            .get("model_providers")
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("ollama"))
            .and_then(|v| v.as_table())
            .and_then(|t| t.get("models"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Convenience wrapper that returns the models list as an io::Result for callers
/// that want a uniform Result-based API.
pub fn read_config_models(config_path: &Path) -> io::Result<Vec<String>> {
    Ok(read_ollama_models_list(config_path))
}

/// Overwrite the recorded models list under [model_providers.ollama].models using toml_edit.
pub fn write_ollama_models_list(config_path: &Path, models: &[String]) -> io::Result<()> {
    let mut doc = read_document(config_path)?;
    {
        let tbl = upsert_provider_ollama(&mut doc);
        let mut arr = toml_edit::Array::new();
        for m in models {
            arr.push(TomlValueEdit::from(m.clone()));
        }
        tbl["models"] = Item::Value(TomlValueEdit::Array(arr));
    }
    write_document(config_path, &doc)
}

/// Write models list via a uniform name expected by higher layers.
pub fn write_config_models(config_path: &Path, models: &[String]) -> io::Result<()> {
    write_ollama_models_list(config_path, models)
}

/// Ensure `[model_providers.ollama]` exists with sensible defaults on disk.
/// Returns true if it created/updated the entry.
pub fn ensure_ollama_provider_entry(codex_home: &Path) -> io::Result<bool> {
    let config_path = codex_home.join("config.toml");
    let mut doc = read_document(&config_path)?;
    let before = doc.to_string();
    let _tbl = upsert_provider_ollama(&mut doc);
    let after = doc.to_string();
    if before != after {
        write_document(&config_path, &doc)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Alias name mirroring the refactor plan wording.
pub fn ensure_provider_entry_and_defaults(codex_home: &Path) -> io::Result<bool> {
    ensure_ollama_provider_entry(codex_home)
}

/// Read whether the provider exists and how many models are recorded under it.
pub fn read_provider_state(config_path: &Path) -> (bool, usize) {
    match std::fs::read_to_string(config_path)
        .ok()
        .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
    {
        Some(toml::Value::Table(root)) => {
            let provider_present = root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("ollama"))
                .map(|_| true)
                .unwrap_or(false);
            let models_count = root
                .get("model_providers")
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("ollama"))
                .and_then(|v| v.as_table())
                .and_then(|t| t.get("models"))
                .and_then(|v| v.as_array())
                .map(|arr| arr.len())
                .unwrap_or(0);
            (provider_present, models_count)
        }
        _ => (false, 0),
    }
}

// ---------- toml_edit helpers ----------

fn read_document(path: &Path) -> io::Result<Document> {
    match std::fs::read_to_string(path) {
        Ok(s) => Document::from_str(&s).map_err(io::Error::other),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Document::new()),
        Err(e) => Err(e),
    }
}

fn write_document(path: &Path, doc: &Document) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, doc.to_string())
}

pub fn upsert_provider_ollama(doc: &mut Document) -> &mut Table {
    // Ensure "model_providers" exists and is a table.
    let needs_init = match doc.get("model_providers") {
        None => true,
        Some(item) => !item.is_table(),
    };
    if needs_init {
        doc.as_table_mut()
            .insert("model_providers", Item::Table(Table::new()));
    }

    // Now, get a mutable reference to the "model_providers" table without `expect`/`unwrap`.
    let providers: &mut Table = {
        // Insert if missing.
        if doc.as_table().get("model_providers").is_none() {
            doc.as_table_mut()
                .insert("model_providers", Item::Table(Table::new()));
        }
        match doc.as_table_mut().get_mut("model_providers") {
            Some(item) => {
                if !item.is_table() {
                    *item = Item::Table(Table::new());
                }
                match item.as_table_mut() {
                    Some(t) => t,
                    None => unreachable!("model_providers was set to a table"),
                }
            }
            None => unreachable!("model_providers should exist after insertion"),
        }
    };

    // Ensure "ollama" exists and is a table.
    let needs_ollama_init = match providers.get("ollama") {
        None => true,
        Some(item) => !item.is_table(),
    };
    if needs_ollama_init {
        providers.insert("ollama", Item::Table(Table::new()));
    }

    // Get a mutable reference to the "ollama" table without `expect`/`unwrap`.
    let tbl: &mut Table = {
        let needs_set = match providers.get("ollama") {
            None => true,
            Some(item) => !item.is_table(),
        };
        if needs_set {
            providers.insert("ollama", Item::Table(Table::new()));
        }
        match providers.get_mut("ollama") {
            Some(item) => {
                if !item.is_table() {
                    *item = Item::Table(Table::new());
                }
                match item.as_table_mut() {
                    Some(t) => t,
                    None => unreachable!("ollama was set to a table"),
                }
            }
            None => unreachable!("ollama should exist after insertion"),
        }
    };

    if !tbl.contains_key("name") {
        tbl["name"] = Item::Value(TomlValueEdit::from("Ollama"));
    }
    if !tbl.contains_key("base_url") {
        tbl["base_url"] = Item::Value(TomlValueEdit::from(DEFAULT_BASE_URL));
    }
    if !tbl.contains_key("wire_api") {
        tbl["wire_api"] = Item::Value(TomlValueEdit::from("chat"));
    }
    tbl
}

pub fn set_ollama_models(doc: &mut Document, models: &[String]) {
    let tbl = upsert_provider_ollama(doc);
    let mut arr = toml_edit::Array::new();
    for m in models {
        arr.push(TomlValueEdit::from(m.clone()));
    }
    tbl["models"] = Item::Value(TomlValueEdit::Array(arr));
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::DocumentMut as Document;

    #[test]
    fn test_upsert_provider_and_models() {
        let mut doc = Document::new();
        let tbl = upsert_provider_ollama(&mut doc);
        assert!(tbl.contains_key("name"));
        assert!(tbl.contains_key("base_url"));
        assert!(tbl.contains_key("wire_api"));
        set_ollama_models(&mut doc, &[String::from("llama3.2:3b")]);
        let root = doc.as_table();
        let mp = match root.get("model_providers").and_then(|i| i.as_table()) {
            Some(t) => t,
            None => panic!("model_providers"),
        };
        let ollama = match mp.get("ollama").and_then(|i| i.as_table()) {
            Some(t) => t,
            None => panic!("ollama"),
        };
        let arr = match ollama.get("models") {
            Some(v) => v,
            None => panic!("models array"),
        };
        assert!(arr.is_array(), "models should be an array");
        let s = doc.to_string();
        assert!(s.contains("model_providers"));
        assert!(s.contains("ollama"));
        assert!(s.contains("models"));
    }
}
