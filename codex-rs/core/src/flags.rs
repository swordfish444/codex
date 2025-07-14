use std::time::Duration;

use env_flags::env_flags;

env_flags! {
    pub OPENAI_DEFAULT_MODEL: &str = "codex-mini-latest";
    pub OPENAI_API_BASE: &str = "https://api.openai.com/v1";

    /// Fallback when the provider-specific key is not set.
    pub OPENAI_API_KEY: Option<&str> = None;
    pub OPENAI_TIMEOUT_MS: Duration = Duration::from_millis(300_000), |value| {
        value.parse().map(Duration::from_millis)
    };
    pub OPENAI_REQUEST_MAX_RETRIES: u64 = 4;
    pub OPENAI_STREAM_MAX_RETRIES: u64 = 10;

    // We generally don't want to disconnect; this updates the timeout to be five minutes
    // which matches the upstream typescript codex impl.
    pub OPENAI_STREAM_IDLE_TIMEOUT_MS: Duration = Duration::from_millis(300_000), |value| {
        value.parse().map(Duration::from_millis)
    };

    /// Fixture path for offline tests (see client.rs).
    pub CODEX_RS_SSE_FIXTURE: Option<&str> = None;
}

// -----------------------------------------------------------------------------
// Test-friendly runtime override helpers
// -----------------------------------------------------------------------------
/// Return the effective retry budget for outbound OpenAI requests.
///
/// The `env_flags!` macro above initialises its values lazily and caches
/// them for the remainder of the process. A number of our unit tests tweak
/// `OPENAI_REQUEST_MAX_RETRIES` *at runtime* (e.g. set to 0/1) to exercise the
/// retry/back‑off logic deterministically. When another test touches the flag
/// first, the cached value "sticks" and later tests silently inherit it,
/// leading to surprising flakes (see #???).
///
/// To make the behaviour deterministic we re‑read the raw environment variable
/// on every call and fall back to the cached default when unset or invalid.
#[inline]
pub fn openai_request_max_retries() -> u64 {
    match std::env::var("OPENAI_REQUEST_MAX_RETRIES") {
        Ok(s) => s.parse::<u64>().unwrap_or(*OPENAI_REQUEST_MAX_RETRIES),
        Err(_) => *OPENAI_REQUEST_MAX_RETRIES,
    }
}
