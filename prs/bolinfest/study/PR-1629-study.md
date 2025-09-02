**DOs**
- **Keep sessions alive on non-terminal events:** After responding to elicitation/approval-style events, use `continue` so the session loop persists.
```
match event {
    EventMsg::ApplyPatchApprovalRequest(_) => {
        outgoing.send_response(id.clone(), result.into()).await;
        continue; // keep session running
    }
    EventMsg::TaskComplete(_) => break,
    _ => { /* other handling */ }
}
```
- **Break only on clear termination signals:** Restrict `break` to `TaskComplete` (or equivalent terminal conditions) to avoid dropping active sessions.
```
EventMsg::TaskComplete(task) => {
    handle_completion(task).await;
    break; // only here
}
```
- **Comment intent, not mechanics:** Replace obvious per-arm comments with a single, high-level note that explains the control-flow contract.
```
/* Session persists across elicitation/approval events; only TaskComplete ends it. */
match event { /* ... */ }
```
- **Keep behavior consistent across similar events:** Ensure all non-terminal, handshake-like events follow the same “respond then continue” pattern.
```
EventMsg::ApplyPatchApprovalRequest(_) => { send(...).await; continue; }
EventMsg::ElicitationResponse(_)       => { send(...).await; continue; }
```

**DON’Ts**
- **Don’t break after non-terminal events:** Using `break` here drops the session prematurely.
```
EventMsg::ApplyPatchApprovalRequest(_) => {
    outgoing.send_response(id.clone(), result.into()).await;
    break; // ❌ prematurely ends session
}
```
- **Don’t add comments that restate the code:** Avoid noise like explaining that `continue` “continues”.
```
outgoing.send_response(id.clone(), result.into()).await;
// Continue, don't break so the session continues. ❌
continue;
```
- **Don’t scatter redundant comments in each match arm:** Prefer one well-placed comment over repetitive, low-value notes.
```
/* Redundant per-arm commentary — remove these. ❌ */
EventMsg::ApplyPatchApprovalRequest(_) => { /* keep going */ continue; }
EventMsg::ElicitationResponse(_)       => { /* keep going */ continue; }
```
- **Don’t leave termination ambiguous:** Mixing `continue` and `break` across similar arms confuses lifecycle semantics—centralize and document the termination rule.