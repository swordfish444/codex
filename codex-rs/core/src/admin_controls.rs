use crate::config_types::AdminAuditEventKind;
use crate::config_types::AdminAuditToml;
use crate::config_types::AdminConfigToml;
use crate::exec::ExecParams;
use crate::exec::SandboxType;
use crate::path_utils::expand_tilde;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use chrono::DateTime;
use chrono::Utc;
use fd_lock::RwLock;
use gethostname::gethostname;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::{self};
use std::path::Path;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tracing::warn;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdminControls {
    pub danger: DangerControls,
    pub audit: Option<AdminAuditConfig>,
    pub pending: Vec<PendingAdminAction>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DangerControls {
    pub disallow_full_access: bool,
    pub allow_with_reason: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdminAuditConfig {
    pub log_file: Option<PathBuf>,
    pub log_endpoint: Option<String>,
    pub log_events: HashSet<AdminAuditEventKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PendingAdminAction {
    Danger(DangerPending),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DangerPending {
    pub source: DangerRequestSource,
    pub requested_sandbox: SandboxPolicy,
    pub requested_approval: AskForApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerRequestSource {
    Startup,
    Resume,
    Approvals,
    ExecCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DangerDecision {
    Allowed,
    RequiresJustification,
    Denied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DangerAuditAction {
    Requested,
    Approved,
    Cancelled,
    Denied,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "audit_kind", rename_all = "snake_case")]
pub enum AdminAuditPayload {
    Danger {
        action: DangerAuditAction,
        justification: Option<String>,
        requested_by: DangerRequestSource,
        sandbox_policy: SandboxPolicy,
        approval_policy: AskForApproval,
    },
    Command {
        command: Vec<String>,
        command_cwd: PathBuf,
        cli_cwd: PathBuf,
        sandbox_type: SandboxType,
        sandbox_policy: SandboxPolicy,
        escalated: bool,
        justification: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminAuditRecord {
    timestamp: DateTime<Utc>,
    username: String,
    hostname: String,
    #[serde(flatten)]
    payload: AdminAuditPayload,
}

impl AdminControls {
    pub fn from_toml(raw: Option<AdminConfigToml>) -> io::Result<Self> {
        let raw = raw.unwrap_or_default();
        let danger = DangerControls {
            disallow_full_access: raw.disallow_danger_full_access.unwrap_or(false),
            allow_with_reason: raw.allow_danger_with_reason.unwrap_or(false),
        };
        let audit = match raw.audit {
            Some(audit_raw) => AdminAuditConfig::from_toml(audit_raw)?,
            None => None,
        };

        Ok(Self {
            danger,
            audit,
            pending: Vec::new(),
        })
    }

    pub fn decision_for_danger(&self) -> DangerDecision {
        if !self.danger.disallow_full_access {
            DangerDecision::Allowed
        } else if self.danger.allow_with_reason {
            DangerDecision::RequiresJustification
        } else {
            DangerDecision::Denied
        }
    }

    pub fn has_pending_danger(&self) -> bool {
        self.pending
            .iter()
            .any(|action| matches!(action, PendingAdminAction::Danger(_)))
    }

    pub fn take_pending_danger(&mut self) -> Option<DangerPending> {
        self.pending
            .extract_if(.., |action| matches!(action, PendingAdminAction::Danger(_)))
            .next()
            .map(|action| match action {
                PendingAdminAction::Danger(pending) => pending,
            })
    }

    pub fn peek_pending_danger(&self) -> Option<&DangerPending> {
        self.pending
            .iter()
            .map(|action| match action {
                PendingAdminAction::Danger(pending) => pending,
            })
            .next()
    }
}

impl AdminAuditConfig {
    pub fn from_toml(raw: AdminAuditToml) -> io::Result<Option<Self>> {
        let AdminAuditToml {
            log_file,
            log_endpoint,
            log_events,
        } = raw;

        let log_file = match log_file {
            Some(path) => {
                let trimmed = path.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(expand_tilde(trimmed)?)
                }
            }
            None => None,
        };

        let log_endpoint = log_endpoint
            .map(|endpoint| endpoint.trim().to_string())
            .filter(|s| !s.is_empty());

        if log_file.is_none() && log_endpoint.is_none() {
            return Ok(None);
        }

        let log_events = log_events.into_iter().collect();

        Ok(Some(Self {
            log_file,
            log_endpoint,
            log_events,
        }))
    }

    pub fn should_log(&self, kind: AdminAuditEventKind) -> bool {
        self.log_events.is_empty() || self.log_events.contains(&kind)
    }
}

impl AdminAuditPayload {
    pub fn kind(&self) -> AdminAuditEventKind {
        match self {
            AdminAuditPayload::Danger { .. } => AdminAuditEventKind::Danger,
            AdminAuditPayload::Command { .. } => AdminAuditEventKind::Command,
        }
    }
}

impl AdminAuditRecord {
    fn new(payload: AdminAuditPayload) -> Self {
        Self {
            timestamp: Utc::now(),
            username: current_username(),
            hostname: current_hostname(),
            payload,
        }
    }
}

pub fn log_admin_event(config: &AdminAuditConfig, payload: AdminAuditPayload) {
    let kind = payload.kind();
    if !config.should_log(kind) {
        return;
    }

    let record = AdminAuditRecord::new(payload);

    if let Some(path) = &config.log_file
        && let Err(err) = append_record_to_file(path, &record)
    {
        warn!(
            "failed to write admin audit event to {}: {err:?}",
            path.display()
        );
    }

    if let Some(endpoint) = &config.log_endpoint {
        if Handle::try_current().is_ok() {
            let endpoint = endpoint.clone();
            tokio::spawn(async move {
                if let Err(err) = send_record_to_endpoint(&endpoint, record).await {
                    warn!("failed to post admin audit event to {endpoint}: {err:?}");
                }
            });
        } else {
            warn!(
                "admin audit HTTP logging requested for {endpoint}, but no async runtime is available",
            );
        }
    }
}

fn append_record_to_file(path: &Path, record: &AdminAuditRecord) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }

    let file = options.open(path)?;
    let mut lock = RwLock::new(file);
    let mut guard = lock.write()?;
    let line = serde_json::to_string(record).map_err(io::Error::other)?;
    guard.write_all(line.as_bytes())?;
    guard.write_all(b"\n")?;
    guard.flush()?;
    Ok(())
}

async fn send_record_to_endpoint(
    endpoint: &str,
    record: AdminAuditRecord,
) -> Result<(), reqwest::Error> {
    Client::new().post(endpoint).json(&record).send().await?;
    Ok(())
}

fn current_username() -> String {
    env_var("USER")
        .or_else(|| env_var("USERNAME"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn current_hostname() -> String {
    gethostname()
        .into_string()
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| env_var("HOSTNAME"))
        .or_else(|| env_var("COMPUTERNAME"))
        .unwrap_or_else(|| "unknown".to_string())
}

fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

pub fn build_danger_audit_payload(
    pending: &DangerPending,
    action: DangerAuditAction,
    justification: Option<String>,
) -> AdminAuditPayload {
    AdminAuditPayload::Danger {
        action,
        justification,
        requested_by: pending.source,
        sandbox_policy: pending.requested_sandbox.clone(),
        approval_policy: pending.requested_approval,
    }
}

pub fn build_command_audit_payload(
    params: &ExecParams,
    sandbox_type: SandboxType,
    sandbox_policy: &SandboxPolicy,
    cli_cwd: &Path,
) -> AdminAuditPayload {
    AdminAuditPayload::Command {
        command: params.command.clone(),
        command_cwd: params.cwd.clone(),
        cli_cwd: cli_cwd.to_path_buf(),
        sandbox_type,
        sandbox_policy: sandbox_policy.clone(),
        escalated: params.with_escalated_permissions.unwrap_or(false),
        justification: params.justification.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn danger_payload_serializes_expected_fields() {
        let pending = DangerPending {
            source: DangerRequestSource::Approvals,
            requested_sandbox: SandboxPolicy::DangerFullAccess,
            requested_approval: AskForApproval::Never,
        };

        let payload = build_danger_audit_payload(
            &pending,
            DangerAuditAction::Requested,
            Some("reason".to_string()),
        );
        let record = AdminAuditRecord::new(payload);
        let value = serde_json::to_value(record).expect("serialize record");

        assert_eq!(
            value.get("audit_kind"),
            Some(&Value::String("danger".to_string()))
        );
        assert_eq!(
            value.get("action"),
            Some(&Value::String("requested".to_string()))
        );
        assert_eq!(
            value.get("requested_by"),
            Some(&Value::String("approvals".to_string()))
        );
        assert_eq!(
            value.get("approval_policy"),
            Some(&Value::String("never".to_string()))
        );
        assert_eq!(
            value.get("sandbox_policy").and_then(|sp| sp.get("mode")),
            Some(&Value::String("danger-full-access".to_string()))
        );
        assert_eq!(
            value.get("justification"),
            Some(&Value::String("reason".to_string()))
        );
    }

    #[test]
    fn command_payload_serializes_expected_fields() {
        let mut env = HashMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());
        let params = ExecParams {
            command: vec!["echo".to_string(), "hello".to_string()],
            cwd: PathBuf::from("/tmp"),
            timeout_ms: Some(1000),
            env,
            with_escalated_permissions: Some(true),
            justification: Some("investigation".to_string()),
        };

        let sandbox_policy = SandboxPolicy::new_workspace_write_policy();
        let payload = build_command_audit_payload(
            &params,
            SandboxType::MacosSeatbelt,
            &sandbox_policy,
            Path::new("/workspace"),
        );
        let record = AdminAuditRecord::new(payload);
        let value = serde_json::to_value(record).expect("serialize record");

        assert_eq!(
            value.get("audit_kind"),
            Some(&Value::String("command".to_string()))
        );
        assert_eq!(
            value.get("command"),
            Some(&serde_json::json!(["echo", "hello"]))
        );
        assert_eq!(
            value.get("command_cwd"),
            Some(&Value::String("/tmp".to_string()))
        );
        assert_eq!(
            value.get("cli_cwd"),
            Some(&Value::String("/workspace".to_string()))
        );
        assert_eq!(
            value.get("sandbox_type"),
            Some(&Value::String("macos-seatbelt".to_string()))
        );
        assert_eq!(
            value.get("sandbox_policy").and_then(|sp| sp.get("mode")),
            Some(&Value::String("workspace-write".to_string()))
        );
        assert_eq!(value.get("escalated"), Some(&Value::Bool(true)));
        assert_eq!(
            value.get("justification"),
            Some(&Value::String("investigation".to_string()))
        );
    }
}
