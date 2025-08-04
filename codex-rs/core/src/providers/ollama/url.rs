use crate::error::Result as CoreResult;

/// Identify whether a base_url points at an OpenAI-compatible root (".../v1").
pub(crate) fn is_openai_compatible_base_url(base_url: &str) -> bool {
    base_url.trim_end_matches('/').ends_with("/v1")
}

/// Convert a provider base_url into the native Ollama host root.
/// For example, "http://localhost:11434/v1" -> "http://localhost:11434".
pub fn base_url_to_host_root(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed
            .trim_end_matches("/v1")
            .trim_end_matches('/')
            .to_string()
    } else {
        trimmed.to_string()
    }
}

/// Variant that considers an explicit WireApi value; provided to centralize
/// host root computation in one place for future extension.
pub fn base_url_to_host_root_with_wire(
    base_url: &str,
    _wire_api: crate::model_provider_info::WireApi,
) -> String {
    base_url_to_host_root(base_url)
}

/// Compute the probe URL to verify if an Ollama server is reachable.
/// If the configured base is OpenAI-compatible (/v1), probe "models", otherwise
/// fall back to the native "/api/tags" endpoint.
pub fn probe_url_for_base(base_url: &str) -> String {
    if is_openai_compatible_base_url(base_url) {
        format!("{}/models", base_url.trim_end_matches('/'))
    } else {
        format!("{}/api/tags", base_url.trim_end_matches('/'))
    }
}

/// Convenience helper to probe an Ollama server given a provider style base URL.
pub async fn probe_ollama_server(base_url: &str) -> CoreResult<bool> {
    let url = probe_url_for_base(base_url);
    let resp = reqwest::Client::new().get(url).send().await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_url_to_host_root() {
        assert_eq!(
            base_url_to_host_root("http://localhost:11434/v1"),
            "http://localhost:11434"
        );
        assert_eq!(
            base_url_to_host_root("http://localhost:11434"),
            "http://localhost:11434"
        );
        assert_eq!(
            base_url_to_host_root("http://localhost:11434/"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn test_probe_url_for_base() {
        assert_eq!(
            probe_url_for_base("http://localhost:11434/v1"),
            "http://localhost:11434/v1/models"
        );
        assert_eq!(
            probe_url_for_base("http://localhost:11434"),
            "http://localhost:11434/api/tags"
        );
    }
}
