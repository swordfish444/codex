use crate::config_types::AdminAuditEventKind;
use crate::config_types::AdminAuditToml;
use crate::config_types::AdminConfigToml;
use crate::exec::ExecParams;
use crate::exec::SandboxType;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use chrono::DateTime;
use chrono::Utc;
use gethostname::gethostname;
use serde::Serialize;
use serde::ser::SerializeMap;
use serde::ser::Serializer;
use std::collections::HashSet;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::io::{self};
use std::path::Path;
use std::path::PathBuf;
use tokio::runtime::Handle;
use tracing::warn;

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

#[derive(Debug, Clone)]
pub enum AdminAuditPayload {
    Danger(DangerAuditDetails),
    Command(CommandAuditDetails),
}

#[derive(Debug, Clone, Serialize)]
pub struct DangerAuditDetails {
    pub action: DangerAuditAction,
    pub justification: Option<String>,
    pub requested_by: DangerRequestSource,
    pub sandbox: String,
    pub approval_policy: AskForApproval,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandAuditDetails {
    pub command: Vec<String>,
    pub command_cwd: String,
    pub cli_cwd: String,
    pub sandbox: String,
    pub sandbox_policy: String,
    pub escalated: bool,
    pub justification: Option<String>,
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
        if let Some(index) = self
            .pending
            .iter()
            .position(|action| matches!(action, PendingAdminAction::Danger(_)))
        {
            match self.pending.remove(index) {
                PendingAdminAction::Danger(pending) => Some(pending),
            }
        } else {
            None
        }
    }

    pub fn peek_pending_danger(&self) -> Option<&DangerPending> {
        self.pending.iter().find_map(|action| match action {
            PendingAdminAction::Danger(pending) => Some(pending),
        })
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
                    Some(expand_path(trimmed)?)
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
            AdminAuditPayload::Danger(_) => AdminAuditEventKind::Danger,
            AdminAuditPayload::Command(_) => AdminAuditEventKind::Command,
        }
    }
}

impl Serialize for AdminAuditPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        match self {
            AdminAuditPayload::Danger(details) => {
                map.serialize_entry("audit_danger", details)?;
            }
            AdminAuditPayload::Command(details) => {
                map.serialize_entry("audit_command", details)?;
            }
        }
        map.end()
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
        && let Err(err) = append_record_to_file(path, &record) {
            warn!(path = %path.display(), ?err, "failed to write admin audit event");
        }

    if let Some(endpoint) = &config.log_endpoint {
        send_record_to_endpoint(endpoint, record);
    }
}

fn append_record_to_file(path: &Path, record: &AdminAuditRecord) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line =
        serde_json::to_string(record).map_err(io::Error::other)?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn send_record_to_endpoint(endpoint: &str, record: AdminAuditRecord) {
    match Handle::try_current() {
        Ok(handle) => {
            let client = reqwest::Client::new();
            let endpoint = endpoint.to_string();
            handle.spawn(async move {
                if let Err(err) = client.post(endpoint).json(&record).send().await {
                    warn!(?err, "failed to post admin audit event");
                }
            });
        }
        Err(_) => {
            warn!("admin audit HTTP logging requested but no async runtime is available");
        }
    }
}

fn expand_path(raw: &str) -> io::Result<PathBuf> {
    if raw == "~" {
        return dirs::home_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not resolve home directory for admin audit log file",
            )
        });
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        let mut home = dirs::home_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "could not resolve home directory for admin audit log file",
            )
        })?;
        if !rest.is_empty() {
            home.push(rest);
        }
        return Ok(home);
    }

    Ok(PathBuf::from(raw))
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

fn sandbox_label(policy: &SandboxPolicy) -> &'static str {
    match policy {
        SandboxPolicy::DangerFullAccess => "danger-full-access",
        SandboxPolicy::ReadOnly => "read-only",
        SandboxPolicy::WorkspaceWrite { .. } => "workspace-write",
    }
}

fn sandbox_type_label(sandbox: SandboxType) -> &'static str {
    match sandbox {
        SandboxType::None => "none",
        SandboxType::MacosSeatbelt => "macos-seatbelt",
        SandboxType::LinuxSeccomp => "linux-seccomp",
    }
}

pub fn build_danger_audit_payload(
    pending: &DangerPending,
    action: DangerAuditAction,
    justification: Option<String>,
) -> AdminAuditPayload {
    AdminAuditPayload::Danger(DangerAuditDetails {
        action,
        justification,
        requested_by: pending.source,
        sandbox: sandbox_label(&pending.requested_sandbox).to_string(),
        approval_policy: pending.requested_approval,
    })
}

pub fn build_command_audit_payload(
    params: &ExecParams,
    sandbox_type: SandboxType,
    sandbox_policy: &SandboxPolicy,
    cli_cwd: &Path,
) -> AdminAuditPayload {
    AdminAuditPayload::Command(CommandAuditDetails {
        command: params.command.clone(),
        command_cwd: params.cwd.display().to_string(),
        cli_cwd: cli_cwd.display().to_string(),
        sandbox: sandbox_type_label(sandbox_type).to_string(),
        sandbox_policy: sandbox_label(sandbox_policy).to_string(),
        escalated: params.with_escalated_permissions.unwrap_or(false),
        justification: params.justification.clone(),
    })
}
