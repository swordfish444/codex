**DOs**

- **Prefer context-rich errors**: wrap variants with structs and implement `Display` to tailor messages.
  ```rust
  #[derive(Debug)]
  pub struct UsageLimitReachedError {
      pub plan_type: Option<String>,
  }

  impl std::fmt::Display for UsageLimitReachedError {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          if let Some(plan) = &self.plan_type && plan == "plus" {
              write!(f, "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing), or wait for limits to reset (every 5h and every week).")
          } else {
              write!(f, "You've hit your usage limit. Limits reset every 5h and every week.")
          }
      }
  }

  #[derive(thiserror::Error, Debug)]
  pub enum CodexErr {
      #[error("{0}")]
      UsageLimitReached(UsageLimitReachedError),
  }
  ```

- **Return actionable messages**: include concrete guidance and links when it helps the user.
  ```rust
  #[derive(thiserror::Error, Debug)]
  pub enum CodexErr {
      #[error("To use Codex with your ChatGPT plan, upgrade to Plus: https://openai.com/chatgpt/pricing.")]
      UsageNotIncluded,
  }
  ```

- **Keep auth internals private**: expose minimal, synchronous accessors that read cached data.
  ```rust
  impl CodexAuth {
      pub fn get_account_id(&self) -> Option<String> {
          self.get_current_token_data().and_then(|t| t.account_id.clone())
      }

      pub fn get_plan_type(&self) -> Option<String> {
          self.get_current_token_data()
              .and_then(|t| t.id_token.chatgpt_plan_type.as_ref().map(|p| p.as_string()))
      }

      fn get_current_auth_json(&self) -> Option<AuthDotJson> {
          #[expect(clippy::unwrap_used)]
          self.auth_dot_json.lock().unwrap().clone()
      }

      fn get_current_token_data(&self) -> Option<TokenData> {
          self.get_current_auth_json().and_then(|t| t.tokens.clone())
      }
  }
  ```

- **Avoid unnecessary async**: make getters sync when they only read memory; update call sites accordingly.
  ```rust
  // Before
  if let Some(account_id) = auth.get_account_id().await {
      req = req.header("chatgpt-account-id", account_id);
  }

  // After
  if let Some(account_id) = auth.get_account_id() {
      req = req.header("chatgpt-account-id", account_id);
  }
  ```

- **Normalize external enum values once**: add a helper to convert to user-facing strings.
  ```rust
  impl PlanType {
      pub fn as_string(&self) -> String {
          match self {
              Self::Known(k) => format!("{k:?}").to_lowercase(),
              Self::Unknown(s) => s.clone(),
          }
      }
  }
  ```

- **Pin message text with unit tests**: verify all branches of formatted errors.
  ```rust
  #[test]
  fn usage_limit_plus_message() {
      let err = UsageLimitReachedError { plan_type: Some("plus".into()) };
      assert_eq!(
          err.to_string(),
          "You've hit your usage limit. Upgrade to Pro (https://openai.com/chatgpt/pricing), or wait for limits to reset (every 5h and every week)."
      );
  }

  #[test]
  fn usage_limit_default_message() {
      let err = UsageLimitReachedError { plan_type: None };
      assert_eq!(
          err.to_string(),
          "You've hit your usage limit. Limits reset every 5h and every week."
      );
  }
  ```

- **Handle specific errors early in flows**: bubble up usage-related errors without retry loops.
  ```rust
  match try_stream_once().await {
      Ok(out) => Ok(out),
      Err(e @ (CodexErr::UsageLimitReached(_) | CodexErr::UsageNotIncluded)) => Err(e),
      Err(e) => retry_stream(e, max_retries).await,
  }?
  ```

- **Map provider responses to typed errors**: attach context (e.g., plan type) when building errors.
  ```rust
  if r#type == "usage_limit_reached" {
      return Err(CodexErr::UsageLimitReached(UsageLimitReachedError {
          plan_type: auth.and_then(|a| a.get_plan_type()),
      }));
  }
  ```

**DON’Ts**

- **Don’t leave punctuation inconsistent**: end sentences consistently across all message branches.
  ```rust
  // Avoid: one branch with period, one without
  write!(f, "Upgrade to Pro (...)")?;
  write!(f, "Limits reset every 5h and every week")?;

  // Do: both with periods
  write!(f, "Upgrade to Pro (...).")?;
  write!(f, "Limits reset every 5h and every week.")?;
  ```

- **Don’t expose internal auth types**: avoid leaking `AuthDotJson` or locking in public APIs.
  ```rust
  // Avoid
  pub async fn get_token_data(&self) -> Result<TokenData, io::Error> {
      let auth = self.auth_dot_json.lock().unwrap().clone(); // leaks internals and lock handling
      /* ... */
  }

  // Do (private helper + public minimal getters)
  fn get_current_auth_json(&self) -> Option<AuthDotJson> { /* ... */ }
  pub fn get_account_id(&self) -> Option<String> { /* ... */ }
  ```

- **Don’t use async when unnecessary**: remove `.await` from getters that only read cached state.
  ```rust
  // Avoid
  let id = auth.get_account_id().await;

  // Do
  let id = auth.get_account_id();
  ```

- **Don’t return bare variants when context helps**: prefer `UsageLimitReached(UsageLimitReachedError { plan_type })` over a context-less `UsageLimitReached`.
  ```rust
  // Avoid
  return Err(CodexErr::UsageLimitReached);

  // Do
  return Err(CodexErr::UsageLimitReached(UsageLimitReachedError { plan_type }));
  ```

- **Don’t make users guess next steps**: include clear guidance (and links) in user-facing errors.
  ```rust
  // Avoid
  #[error("Usage not included with the plan")]
  UsageNotIncluded,

  // Do
  #[error("To use Codex with your ChatGPT plan, upgrade to Plus: https://openai.com/chatgpt/pricing.")]
  UsageNotIncluded,
  ```