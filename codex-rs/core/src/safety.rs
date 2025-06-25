use std::collections::HashSet;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::ApplyPatchFileChange;

use crate::exec::SandboxType;
use crate::is_safe_command::is_known_safe_command;
use crate::protocol::AskForApproval;
use crate::protocol::SandboxPolicy;
use crate::config::AutoAllowPredicate;

#[derive(Debug)]
pub enum SafetyCheck {
    AutoApprove { sandbox_type: SandboxType },
    AskUser,
    Reject { reason: String },
}

pub fn assess_patch_safety(
    action: &ApplyPatchAction,
    policy: AskForApproval,
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> SafetyCheck {
    if action.is_empty() {
        return SafetyCheck::Reject {
            reason: "empty patch".to_string(),
        };
    }

    match policy {
        AskForApproval::OnFailure | AskForApproval::AutoEdit | AskForApproval::Never => {
            // Continue to see if this can be auto-approved.
        }
        // TODO(ragona): I'm not sure this is actually correct? I believe in this case
        // we want to continue to the writable paths check before asking the user.
        AskForApproval::UnlessAllowListed => {
            return SafetyCheck::AskUser;
        }
    }

    if is_write_patch_constrained_to_writable_paths(action, writable_roots, cwd) {
        SafetyCheck::AutoApprove {
            sandbox_type: SandboxType::None,
        }
    } else if policy == AskForApproval::OnFailure {
        // Only auto‑approve when we can actually enforce a sandbox. Otherwise
        // fall back to asking the user because the patch may touch arbitrary
        // paths outside the project.
        match get_platform_sandbox() {
            Some(sandbox_type) => SafetyCheck::AutoApprove { sandbox_type },
            None => SafetyCheck::AskUser,
        }
    } else if policy == AskForApproval::Never {
        SafetyCheck::Reject {
            reason: "writing outside of the project; rejected by user approval settings"
                .to_string(),
        }
    } else {
        SafetyCheck::AskUser
    }
}

pub fn assess_command_safety(
    command: &[String],
    approval_policy: AskForApproval,
    sandbox_policy: &SandboxPolicy,
    approved: &HashSet<Vec<String>>,
) -> SafetyCheck {
    let approve_without_sandbox = || SafetyCheck::AutoApprove {
        sandbox_type: SandboxType::None,
    };

    // Previously approved or allow-listed commands
    // All approval modes allow these commands to continue without sandboxing
    if is_known_safe_command(command) || approved.contains(command) {
        // TODO(ragona): I think we should consider running even these inside the sandbox, but it's
        // a change in behavior so I'm keeping it at parity with upstream for now.
        return approve_without_sandbox();
    }

    // Command was not known-safe or allow-listed
    if sandbox_policy.is_unrestricted() {
        approve_without_sandbox()
    } else {
        match get_platform_sandbox() {
            // We have a sandbox, so we can approve the command in all modes
            Some(sandbox_type) => SafetyCheck::AutoApprove { sandbox_type },
            None => {
                // We do not have a sandbox, so we need to consider the approval policy
                match approval_policy {
                    // Never is our "non-interactive" mode; it must automatically reject
                    AskForApproval::Never => SafetyCheck::Reject {
                        reason: "auto-rejected by user approval settings".to_string(),
                    },
                    // Otherwise, we ask the user for approval
                    _ => SafetyCheck::AskUser,
                }
            }
        }
    }
}

pub fn get_platform_sandbox() -> Option<SandboxType> {
    if cfg!(target_os = "macos") {
        Some(SandboxType::MacosSeatbelt)
    } else if cfg!(target_os = "linux") {
        Some(SandboxType::LinuxSeccomp)
    } else {
        None
    }
}

/// Vote returned by auto-approval predicate scripts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoAllowVote {
    /// Script approved the command.
    Allow,
    /// Script denied the command.
    Deny,
    /// Script had no opinion (or errored).
    NoOpinion,
}

/// Evaluate user-configured auto-approval predicates for the given command.
/// Invokes each script in order, passing the full candidate command as the only argument.
/// Returns the first `Allow` or `Deny` vote, or `NoOpinion` if none asserted.
pub fn evaluate_auto_allow_predicates(
    command: &[String],
    predicates: &[AutoAllowPredicate],
) -> AutoAllowVote {
    if predicates.is_empty() {
        return AutoAllowVote::NoOpinion;
    }
    let cmd_text = command.join(" ");
    for pred in predicates {
        let output = std::process::Command::new(&pred.script)
            .arg(&cmd_text)
            .output();
        let vote = match output {
            Ok(output) if output.status.success() => match String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "allow" => AutoAllowVote::Allow,
                "deny" => AutoAllowVote::Deny,
                "no-opinion" => AutoAllowVote::NoOpinion,
                _ => AutoAllowVote::NoOpinion,
            },
            _ => AutoAllowVote::NoOpinion,
        };
        if vote == AutoAllowVote::Deny {
            return AutoAllowVote::Deny;
        }
        if vote == AutoAllowVote::Allow {
            return AutoAllowVote::Allow;
        }
    }
    AutoAllowVote::NoOpinion
}

fn is_write_patch_constrained_to_writable_paths(
    action: &ApplyPatchAction,
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> bool {
    // Early‑exit if there are no declared writable roots.
    if writable_roots.is_empty() {
        return false;
    }

    // Normalize a path by removing `.` and resolving `..` without touching the
    // filesystem (works even if the file does not exist).
    fn normalize(path: &Path) -> Option<PathBuf> {
        let mut out = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => { /* skip */ }
                other => out.push(other.as_os_str()),
            }
        }
        Some(out)
    }

    // Determine whether `path` is inside **any** writable root. Both `path`
    // and roots are converted to absolute, normalized forms before the
    // prefix check.
    let is_path_writable = |p: &PathBuf| {
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            cwd.join(p)
        };
        let abs = match normalize(&abs) {
            Some(v) => v,
            None => return false,
        };

        writable_roots.iter().any(|root| {
            let root_abs = if root.is_absolute() {
                root.clone()
            } else {
                normalize(&cwd.join(root)).unwrap_or_else(|| cwd.join(root))
            };

            abs.starts_with(&root_abs)
        })
    };

    for (path, change) in action.changes() {
        match change {
            ApplyPatchFileChange::Add { .. } | ApplyPatchFileChange::Delete => {
                if !is_path_writable(path) {
                    return false;
                }
            }
            ApplyPatchFileChange::Update { move_path, .. } => {
                if !is_path_writable(path) {
                    return false;
                }
                if let Some(dest) = move_path {
                    if !is_path_writable(dest) {
                        return false;
                    }
                }
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    use crate::config::AutoAllowPredicate;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn test_writable_roots_constraint() {
        let cwd = std::env::current_dir().unwrap();
        let parent = cwd.parent().unwrap().to_path_buf();

        // Helper to build a single‑entry map representing a patch that adds a
        // file at `p`.
        let make_add_change = |p: PathBuf| ApplyPatchAction::new_add_for_test(&p, "".to_string());

        let add_inside = make_add_change(cwd.join("inner.txt"));
        let add_outside = make_add_change(parent.join("outside.txt"));

        assert!(is_write_patch_constrained_to_writable_paths(
            &add_inside,
            &[PathBuf::from(".")],
            &cwd,
        ));

        let add_outside_2 = make_add_change(parent.join("outside.txt"));
        assert!(!is_write_patch_constrained_to_writable_paths(
            &add_outside_2,
            &[PathBuf::from(".")],
            &cwd,
        ));

        // With parent dir added as writable root, it should pass.
        assert!(is_write_patch_constrained_to_writable_paths(
            &add_outside,
            &[PathBuf::from("..")],
            &cwd,
        ))
    }

    #[test]
    fn test_evaluate_auto_allow_predicates_votes() {
        let dir = tempdir().unwrap();
        let allow_script = dir.path().join("allow.sh");
        std::fs::write(&allow_script, "#!/usr/bin/env bash\necho allow\n").unwrap();
        let mut perms = std::fs::metadata(&allow_script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&allow_script, perms).unwrap();

        let deny_script = dir.path().join("deny.sh");
        std::fs::write(&deny_script, "#!/usr/bin/env bash\necho deny\n").unwrap();
        let mut perms2 = std::fs::metadata(&deny_script).unwrap().permissions();
        perms2.set_mode(0o755);
        std::fs::set_permissions(&deny_script, perms2).unwrap();

        // Allow script should return Allow
        let preds = vec![AutoAllowPredicate { script: allow_script.to_string_lossy().into() }];
        let vote = evaluate_auto_allow_predicates(&["cmd".to_string()], &preds);
        assert_eq!(vote, AutoAllowVote::Allow);

        // Deny script takes precedence over allow
        let preds2 = vec![AutoAllowPredicate { script: deny_script.to_string_lossy().into() },
                          AutoAllowPredicate { script: allow_script.to_string_lossy().into() }];
        let vote2 = evaluate_auto_allow_predicates(&["cmd".to_string()], &preds2);
        assert_eq!(vote2, AutoAllowVote::Deny);

        // No predicates yields NoOpinion
        let vote3 = evaluate_auto_allow_predicates(&["cmd".to_string()], &[]);
        assert_eq!(vote3, AutoAllowVote::NoOpinion);
    }

    #[test]
    fn test_evaluate_auto_allow_predicates_various_no_opinion_cases() {
        let dir = tempdir().unwrap();
        // Script that explicitly returns no-opinion
        let noop_script = dir.path().join("noop.sh");
        std::fs::write(&noop_script, "#!/usr/bin/env bash\necho no-opinion\n").unwrap();
        let mut perms = std::fs::metadata(&noop_script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&noop_script, perms).unwrap();

        // Script that returns unknown output
        let unknown_script = dir.path().join("unknown.sh");
        std::fs::write(&unknown_script, "#!/usr/bin/env bash\necho maybe\n").unwrap();
        let mut perms2 = std::fs::metadata(&unknown_script).unwrap().permissions();
        perms2.set_mode(0o755);
        std::fs::set_permissions(&unknown_script, perms2).unwrap();

        // Script that exits with an error
        let error_script = dir.path().join("error.sh");
        std::fs::write(&error_script, "#!/usr/bin/env bash\nexit 1\n").unwrap();
        let mut perms3 = std::fs::metadata(&error_script).unwrap().permissions();
        perms3.set_mode(0o755);
        std::fs::set_permissions(&error_script, perms3).unwrap();

        // All scripts no-opinion or error yields NoOpinion
        let preds = vec![
            AutoAllowPredicate { script: noop_script.to_string_lossy().into() },
            AutoAllowPredicate { script: unknown_script.to_string_lossy().into() },
            AutoAllowPredicate { script: error_script.to_string_lossy().into() },
        ];
        let vote = evaluate_auto_allow_predicates(&["cmd".to_string()], &preds);
        assert_eq!(vote, AutoAllowVote::NoOpinion);
    }

    #[test]
    fn test_evaluate_auto_allow_predicates_short_circuits_after_no_opinion() {
        let dir = tempdir().unwrap();
        // First script no-opinion
        let noop_script = dir.path().join("noop2.sh");
        std::fs::write(&noop_script, "#!/usr/bin/env bash\necho no-opinion\n").unwrap();
        let mut perms = std::fs::metadata(&noop_script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&noop_script, perms).unwrap();

        // Second script allow
        let allow_script = dir.path().join("allow2.sh");
        std::fs::write(&allow_script, "#!/usr/bin/env bash\necho allow\n").unwrap();
        let mut perms2 = std::fs::metadata(&allow_script).unwrap().permissions();
        perms2.set_mode(0o755);
        std::fs::set_permissions(&allow_script, perms2).unwrap();

        let preds = vec![
            AutoAllowPredicate { script: noop_script.to_string_lossy().into() },
            AutoAllowPredicate { script: allow_script.to_string_lossy().into() },
        ];
        let vote = evaluate_auto_allow_predicates(&["cmd".to_string()], &preds);
        assert_eq!(vote, AutoAllowVote::Allow);
    }
}
