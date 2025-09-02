**PR #1865 Review Takeaways**

**DOs**
- **Use match for enums:** Prefer exhaustive matches over ad-hoc conditionals to force decisions when new variants are added.
```rust
match approval_policy {
    AskForApproval::Never | AskForApproval::OnRequest => {
        // Early return to the model on sandbox error
    }
    AskForApproval::UnlessTrusted | AskForApproval::OnFailure => {
        // Continue with escalation flow
    }
}
```

- **Encapsulate policy checks:** Move repeated branching logic into methods on the enum to centralize behavior.
```rust
impl AskForApproval {
    pub fn early_out_on_sandbox_error(self) -> bool {
        matches!(self, AskForApproval::Never | AskForApproval::OnRequest)
    }
}

// Usage
if approval_policy.early_out_on_sandbox_error() {
    // Early return
}
```

- **Add explanatory text around code blocks:** In docs, insert context between examples to explain when to choose each option.
```md
If you want to trust only known-safe commands, use "untrusted":
```toml
approval_policy = "untrusted"
```

If you want to be prompted only when a sandboxed command fails, use "on-failure":
```toml
approval_policy = "on-failure"
```

To let the model decide when to escalate, use "on-request":
```toml
approval_policy = "on-request"
```
```

**DON’Ts**
- **Don’t chain equality checks on enums:** Avoid multiple `==` comparisons that are easy to miss when variants change.
```rust
// Avoid
if approval_policy == AskForApproval::Never {
    // ...
} else if approval_policy == AskForApproval::OnRequest {
    // ...
} else {
    // ...
}
```

- **Don’t use a wildcard arm that hides new variants:** List variants explicitly so additions cause a compile-time prompt to handle them.
```rust
// Avoid
match approval_policy {
    AskForApproval::UnlessTrusted | AskForApproval::OnFailure => { /* ... */ }
    _ => { /* catches Never, OnRequest, and any future variants unintentionally */ }
}
```

- **Don’t stack empty or context-free code fences:** Never place fenced blocks back-to-back without narrative; avoid empty fences.
```md
// Avoid (no explanation and an empty block)
```toml
approval_policy = "untrusted"
```
```toml
```
```