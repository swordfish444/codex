#![deny(clippy::unwrap_used, clippy::expect_used)]

pub use api::ApplyOutcome;
pub use api::ApplyStatus;
pub use api::CloudBackend;
pub use api::Error;
pub use api::Result;
pub use api::TaskId;
pub use api::TaskStatus;
pub use api::TaskSummary;
use codex_cloud_tasks_api as api;

#[cfg(feature = "mock")]
mod mock;

#[cfg(feature = "online")]
mod http;

#[cfg(feature = "mock")]
pub use mock::MockClient;

#[cfg(feature = "online")]
pub use http::HttpClient;

// Reusable apply engine (git apply runner and helpers)
pub mod patch_apply;
