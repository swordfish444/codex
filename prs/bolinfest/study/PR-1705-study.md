**PR #1705 — Apply Patch via Sandbox: DOs and DON’Ts**

**DOs**
- Bold: Route auto-approved patches through sandbox: Use `InternalApplyPatchInvocation::DelegateToExec` when `SafetyCheck::AutoApprove` is returned so writes happen under the configured sandbox.
- Bold: Keep programmatic path for explicit approvals: If the user explicitly approves, return `InternalApplyPatchInvocation::Output` from `core::apply_patch` and apply in-process.
- Bold: Use a single canonical arg: Reference `codex_core::CODEX_APPLY_PATCH_ARG1` instead of hardcoding `"--codex-run-as-apply-patch"`.
- Bold: Execute via codex binary: Build `ExecParams` with `std::env::current_exe()` + `{CODEX_APPLY_PATCH_ARG1, patch}` and the action’s `cwd`.
- Bold: Assess as untrusted: For delegated `apply_patch`, call `assess_safety_for_untrusted_command` to select sandboxing independent of prior approvals.
- Bold: Show sanitized command: Emit `ExecCommandBegin` with `command_for_display = ["apply_patch", patch]` and the effective `cwd`.
- Bold: Handle non-UTF8/lookup failures gracefully: On failure to derive codex path, return a non-panic error response.
- Bold: Preserve raw patch and hunks: Make parsing return `ApplyPatchArgs { patch, hunks }`; pass `patch` (sans heredoc wrapper) to the exec invocation; use `hunks` for verification and previews.
- Bold: Inline variables in `format!`: Embed variables directly inside `{}` in strings.
- Bold: Update tests and call sites: Access `parse_patch(...).unwrap().hunks` in tests and logic.

```rust
// Detect apply_patch and decide programmatic vs sandboxed execution
let action_for_exec = match maybe_parse_apply_patch_verified(&params.command, &params.cwd) {
    MaybeApplyPatchVerified::Body(changes) => match apply_patch::apply_patch(sess, &sub_id, &call_id, changes).await {
        InternalApplyPatchInvocation::Output(item) => return item,           // explicit approval → in-process
        InternalApplyPatchInvocation::DelegateToExec(action) => Some(action) // auto-approved → sandboxed exec
    },
    MaybeApplyPatchVerified::CorrectnessError(e) => {
        return ResponseInputItem::FunctionCallOutput {
            call_id,
            output: FunctionCallOutputPayload { content: format!("error: {e:#}"), success: None },
        };
    }
    _ => None,
};

// Build ExecParams for delegated apply_patch
let (params, safety, command_for_display) = match action_for_exec {
    Some(ApplyPatchAction { patch, cwd, .. }) => {
        let path_to_codex = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let Some(path_to_codex) = path_to_codex else {
            return ResponseInputItem::FunctionCallOutput {
                call_id,
                output: FunctionCallOutputPayload {
                    content: "failed to determine path to codex executable".to_string(),
                    success: None,
                },
            };
        };

        let params = ExecParams {
            command: vec![path_to_codex, CODEX_APPLY_PATCH_ARG1.to_string(), patch.clone()],
            cwd,
            timeout_ms: params.timeout_ms,
            env: std::collections::HashMap::new(),
        };
        let safety = assess_safety_for_untrusted_command(sess.approval_policy, &sess.sandbox_policy);
        (params, safety, vec!["apply_patch".to_string(), patch])
    }
    None => {
        let safety = {
            let state = sess.state.lock().unwrap();
            assess_command_safety(&params.command, sess.approval_policy, &sess.sandbox_policy, &state.approved_commands)
        };
        let command_for_display = params.command.clone();
        (params, safety, command_for_display)
    }
};

// Emit sanitized begin event
sess.notify_exec_command_begin(&sub_id, &call_id, command_for_display.clone(), &params.cwd).await;
```

```rust
// arg0 dispatch uses constant
use codex_core::CODEX_APPLY_PATCH_ARG1;
if argv1 == CODEX_APPLY_PATCH_ARG1 {
    // ... run apply_patch subcommand ...
}
```

```rust
// Parser returns both raw patch and hunks
let parsed = parse_patch(&patch_text)?;
let hunks = parsed.hunks;   // for verification and previews
let patch = parsed.patch;   // clean text (no heredoc wrapper) for exec
```

```rust
// Keep format! inlined variables when constructing minimal patches
let patch = format!(
    r#"*** Begin Patch
*** Update File: {filename}
@@
+ {content}
*** End Patch"#,
);
```

**DON’Ts**
- Bold: Don’t apply auto-approved patches in-process: Avoid trusting path checks alone; symlinks/hard links can bypass them—always sandbox.
- Bold: Don’t hardcode special flags: Never write `"--codex-run-as-apply-patch"` inline; use `CODEX_APPLY_PATCH_ARG1`.
- Bold: Don’t panic on path issues: Non-UTF8 or lookup failures for the codex binary must not crash; return a structured error.
- Bold: Don’t leak host binary paths in events: Use `["apply_patch", patch]`, not the codex binary path, in `ExecCommandBegin`.
- Bold: Don’t pass heredoc wrappers to exec: Strip to the canonical `patch` string and pass only that.
- Bold: Don’t drop `cwd`: Ensure the delegated exec uses the action’s `cwd` (where relative paths were resolved).
- Bold: Don’t bypass safety plumbing: Keep `(params, safety, command_for_display)` flow consistent with normal exec and reuse error handling paths.

```rust
// Bad: in-process write on auto-approve (vulnerable to hard-link exploits)
// let _ = apply_patch_in_process(action);

// Good: delegate auto-approved patches to sandboxed exec instead
// InternalApplyPatchInvocation::DelegateToExec(action)
```