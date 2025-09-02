**DOs**
- **Avoid unnecessary clones in comparisons:** Compare borrowed values directly; cloning allocates needlessly.
  ```rust
  // Good
  let is_current = preset.model == current_model && preset.effort == current_effort;
  ```
- **Log state changes with before/after context:** Include both previous and new values to make audits useful.
  ```rust
  let prev_model = current_model.clone();
  let prev_effort = current_effort;
  let model_slug = preset.model.to_string();
  let effort = preset.effort;

  let actions: Vec<SelectionAction> = vec![Box::new(move |tx| {
      tx.send(AppEvent::UpdateModel(model_slug.clone()));
      tx.send(AppEvent::UpdateReasoningEffort(effort));
      tracing::info!(
          "Model change: prev={}, new={}; Effort: prev={}, new={}",
          prev_model, model_slug, prev_effort, effort
      );
  })];
  ```
- **Clone only at ownership boundaries:** Clone when an API requires owned data; borrow for formatting/logging.
  ```rust
  tx.send(AppEvent::UpdateModel(model_slug.clone())); // needs owned String
  tracing::info!("New model: {}", model_slug);        // borrow; no clone
  ```
- **Use concise, consistent log phrasing:** Keep keys stable and values inline for grepability.
  ```rust
  tracing::info!("Model change: prev={}, new={}; Effort: prev={}, new={}", prev_model, model_slug, prev_effort, effort);
  ```

**DON'Ts**
- **Don’t clone just to compare:** Cloning for equality checks wastes CPU and allocates.
  ```rust
  // Bad
  let is_current = preset.model == current_model.clone() && preset.effort == current_effort;
  ```
- **Don’t clone just to log:** `tracing`/`format!` take references; cloning adds no value.
  ```rust
  // Bad
  tracing::info!("New model: {}", model_slug.clone());
  ```
- **Don’t shadow when capturing previous state:** Use a distinct name instead of reusing the same identifier.
  ```rust
  // Bad
  let current_model = current_model.clone();

  // Good
  let prev_model = current_model.clone();
  ```
- **Don’t log only the new value:** Without the previous value, change events are hard to interpret.
  ```rust
  // Bad
  tracing::info!("New model: {}", model_slug);

  // Better
  tracing::info!("Model change: prev={}, new={}", prev_model, model_slug);
  ```