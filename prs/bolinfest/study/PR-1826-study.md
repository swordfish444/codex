**DOs**
- Use provider helpers for URLs and requests: centralize auth, headers, and query params.
  ```rust
  // Good: delegate to provider
  let mut req = provider.create_request_builder(&client, &auth).await?
      .header("OpenAI-Beta", "responses=experimental")
      .header("session_id", session_id.to_string())
      .header(reqwest::header::ACCEPT, "text/event-stream")
      .json(&payload);
  ```
- Pass auth into URL construction so base URL (ChatGPT vs API) is chosen correctly.
  ```rust
  trace!("POST to {}: {}", provider.get_full_url(&auth), serde_json::to_string(&payload)?);
  ```
- Inline variables in format/trace macros instead of positional args.
  ```rust
  let url = provider.get_full_url(&auth);
  trace!("POST to {url}: {}", serde_json::to_string(&payload)?);
  ```
- Use if-let chains to add headers conditionally and concisely.
  ```rust
  if let Some(a) = auth.as_ref()
      && a.mode == AuthMode::ChatGPT
      && let Some(account_id) = a.get_account_id().await
  {
      req = req.header("chatgpt-account-id", account_id);
  }
  ```
- Use Cow to fall back to provider-derived auth without ownership headaches.
  ```rust
  use std::borrow::Cow;
  let auth: Cow<'_, Option<CodexAuth>> = if auth.is_some() {
      Cow::Borrowed(&auth)
  } else {
      Cow::Owned(provider.get_fallback_auth()?)
  };
  let url = provider.get_full_url(&auth);
  let mut req = client.post(url);
  if let Some(a) = auth.as_ref() {
      req = req.bearer_auth(a.get_token().await?);
  }
  ```
- Keep provider internals private when possible; expose only what callers need.
  ```rust
  // In ModelProviderInfo
  fn get_query_string(&self) -> String { /* ... */ }    // private
  pub(crate) fn get_full_url(&self, auth: &Option<CodexAuth>) -> String { /* ... */ }
  ```
- Prefer simple, exact assertions in tests; rely on MockServer’s expectations.
  ```rust
  let existing = if cfg!(windows) { "USERNAME" } else { "USER" };
  Mock::given(method("POST"))
      .and(path("/openai/responses"))
      .and(query_param("api-version", "2025-04-01-preview"))
      .and(header_regex("Custom-Header", "Value"))
      .and(header_regex("Authorization",
           &format!("^Bearer {}$", std::env::var(existing).unwrap())))
      .respond_with(first)
      .expect(1)
      .mount(&server)
      .await;
  ```
- Make cross-platform env-dependent tests robust by choosing vars that exist.
  ```rust
  let env_key = if cfg!(windows) { "USERNAME" } else { "USER" };
  let provider = ModelProviderInfo { env_key: Some(env_key.into()), ..provider };
  ```

**DON’Ts**
- Don’t hand-build URLs or query strings in clients; don’t recompute base URLs per attempt.
  ```rust
  // Avoid
  // let url = format!("{base_url}/responses{}", provider.get_query_string());
  // client.post(url)...
  ```
- Don’t fetch or inject API keys in the client layer when the provider can supply/fallback them.
  ```rust
  // Avoid
  // let token = auth.get_token().await?;
  // .bearer_auth(&token)
  ```
- Don’t keep redundant match arms or code paths that produce the same output.
  ```rust
  // Avoid duplicate branches yielding identical format strings
  match (wire_api, chatgpt_mode) {
      (WireApi::Responses, true) => format!("{base}/responses{q}"),
      (WireApi::Responses, false) => format!("{base}/responses{q}"), // redundant
      _ => unimplemented!(),
  }
  ```
- Don’t expose low-level helpers like get_query_string publicly without necessity.
  ```rust
  // Avoid
  // pub fn get_query_string(&self) -> String
  ```
- Don’t use unsafe env mutation in tests; avoid std::env::set_var.
  ```rust
  // Avoid
  // unsafe { std::env::set_var("TEST_API_KEY_ENV_VAR", "value"); }
  ```
- Don’t skip validating Authorization headers in HTTP tests when auth should be present.
  ```rust
  // Avoid
  // .and(header_regex("Authorization", "Bearer .+")) // prefer exact match when possible
  ```