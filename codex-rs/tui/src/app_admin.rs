use crate::app::App;
use codex_common::approval_presets::ApprovalPreset;
use codex_core::admin_controls::DangerAuditAction;
use codex_core::admin_controls::DangerDecision;
use codex_core::admin_controls::DangerPending;
use codex_core::admin_controls::DangerRequestSource;
use codex_core::admin_controls::PendingAdminAction;
use codex_core::admin_controls::build_danger_audit_payload;
use codex_core::admin_controls::log_admin_event;
use codex_core::protocol::AskForApproval;
use codex_core::protocol::Op;
use codex_core::protocol::SandboxPolicy;
use color_eyre::eyre::Result;

impl App {
    pub(crate) fn handle_apply_approval_preset(&mut self, preset: ApprovalPreset) -> Result<()> {
        self.cancel_existing_pending_requests();

        let ApprovalPreset {
            approval, sandbox, ..
        } = preset;

        match sandbox {
            SandboxPolicy::DangerFullAccess => match self.config.admin.decision_for_danger() {
                DangerDecision::Allowed => {
                    let pending = DangerPending {
                        source: DangerRequestSource::Approvals,
                        requested_sandbox: SandboxPolicy::DangerFullAccess,
                        requested_approval: approval,
                    };
                    self.log_danger_event(&pending, DangerAuditAction::Approved, None);
                    self.apply_sandbox_and_approval(approval, SandboxPolicy::DangerFullAccess);
                }
                DangerDecision::RequiresJustification => {
                    let pending = DangerPending {
                        source: DangerRequestSource::Approvals,
                        requested_sandbox: SandboxPolicy::DangerFullAccess,
                        requested_approval: approval,
                    };
                    self.log_danger_event(&pending, DangerAuditAction::Requested, None);
                    self.push_pending_danger(pending.clone());
                    self.chat_widget.prompt_for_danger_justification(pending);
                }
                DangerDecision::Denied => {
                    let pending = DangerPending {
                        source: DangerRequestSource::Approvals,
                        requested_sandbox: SandboxPolicy::DangerFullAccess,
                        requested_approval: approval,
                    };
                    self.log_danger_event(&pending, DangerAuditAction::Denied, None);
                    self.chat_widget.add_error_message(
                        "Full access is disabled by your administrator.".to_string(),
                    );
                }
            },
            other => {
                self.apply_sandbox_and_approval(approval, other);
            }
        }

        Ok(())
    }

    pub(crate) fn handle_danger_justification_submission(
        &mut self,
        justification: String,
    ) -> Result<()> {
        let justification = justification.trim();
        if justification.is_empty() {
            self.chat_widget.add_error_message(
                "Please provide a justification before enabling full access.".to_string(),
            );
            return Ok(());
        }

        let Some(pending) = self.chat_widget.take_pending_danger() else {
            return Ok(());
        };
        if let Some(internal) = self.drop_pending_from_configs() {
            debug_assert_eq!(internal, pending);
        }

        self.log_danger_event(
            &pending,
            DangerAuditAction::Approved,
            Some(justification.to_string()),
        );

        let DangerPending {
            requested_approval,
            requested_sandbox,
            ..
        } = pending;
        self.apply_sandbox_and_approval(requested_approval, requested_sandbox);
        self.chat_widget.add_info_message(
            "Full access enabled.".to_string(),
            Some("Justification has been logged.".to_string()),
        );
        Ok(())
    }

    pub(crate) fn handle_danger_justification_cancelled(&mut self) -> Result<()> {
        self.cancel_existing_pending_requests();

        let approval_label = self.config.approval_policy.to_string();
        let sandbox_label = self.config.sandbox_policy.to_string();

        self.chat_widget.add_info_message(
            format!(
                "Full access remains disabled. Current approval policy `{approval_label}`, sandbox `{sandbox_label}`."
            ),
            None,
        );

        Ok(())
    }

    pub(crate) fn process_pending_admin_controls(&mut self) {
        while let Some(pending) = self.drop_pending_from_configs() {
            self.chat_widget.prompt_for_danger_justification(pending);
        }
    }

    fn apply_sandbox_and_approval(&mut self, approval: AskForApproval, sandbox: SandboxPolicy) {
        self.chat_widget.submit_op(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: Some(approval),
            sandbox_policy: Some(sandbox.clone()),
            model: None,
            effort: None,
            summary: None,
        });
        self.chat_widget.set_approval_policy(approval);
        self.chat_widget.set_sandbox_policy(sandbox.clone());
        self.config.approval_policy = approval;
        self.config.sandbox_policy = sandbox;
    }

    fn push_pending_danger(&mut self, pending: DangerPending) {
        self.config
            .admin
            .pending
            .push(PendingAdminAction::Danger(pending.clone()));
        self.chat_widget
            .config_mut()
            .admin
            .pending
            .push(PendingAdminAction::Danger(pending));
    }

    fn cancel_existing_pending_requests(&mut self) {
        let mut logged = false;

        if let Some(previous) = self.config.admin.take_pending_danger() {
            self.log_danger_event(&previous, DangerAuditAction::Cancelled, None);
            logged = true;
        }

        if let Some(previous) = self.chat_widget.config_mut().admin.take_pending_danger() {
            if !logged {
                self.log_danger_event(&previous, DangerAuditAction::Cancelled, None);
                logged = true;
            }
        }

        if let Some(previous) = self.chat_widget.take_pending_danger() {
            if !logged {
                self.log_danger_event(&previous, DangerAuditAction::Cancelled, None);
            }
        }
    }

    fn drop_pending_from_configs(&mut self) -> Option<DangerPending> {
        let config_pending = self.config.admin.take_pending_danger();
        let widget_pending = self.chat_widget.config_mut().admin.take_pending_danger();
        config_pending.or(widget_pending)
    }

    fn log_danger_event(
        &self,
        pending: &DangerPending,
        action: DangerAuditAction,
        justification: Option<String>,
    ) {
        if let Some(audit) = self.config.admin.audit.as_ref() {
            log_admin_event(
                audit,
                build_danger_audit_payload(pending, action, justification),
            );
        }
    }
}
