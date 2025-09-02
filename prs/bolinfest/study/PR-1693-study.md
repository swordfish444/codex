**DOs**
- **Specify api-version:** Always include Azure’s required `api-version` in `query_params`.
```toml
[model_providers.azure]
name = "Azure"
# Copy the endpoint from Azure AI Foundry, then extract base_url and api-version.
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
```

- **Match wire_api to model:** Use `responses` for models like `codex-mini`; use `chat` for models like `gpt-4o`, `o3-mini`, `gpt-4.1`, or `o4-mini`.
```toml
# responses API (e.g., codex-mini)
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
wire_api = "responses"
```
```toml
# chat API (e.g., gpt-4o, o3-mini, gpt-4.1, o4-mini)
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT_NAME"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
wire_api = "chat"
```

- **Use the correct base_url path:** For `responses`, omit `/deployments/...`; for `chat`, include `/deployments/YOUR_DEPLOYMENT_NAME` (see examples above).

- **Place comments above settings and wrap lines:** Keep long explanations as separate comments above the relevant keys; wrap at ~80–100 columns to avoid horizontal scrolling.
```toml
# Use the responses API for codex-mini.
# Keep long explanations in wrapped comment lines above the setting.
wire_api = "responses"
```

- **Verify readability in GitHub:** Use “Display the rich diff” to ensure examples don’t require horizontal scrolling.


**DON’Ts**
- **Don’t omit api-version:** Azure requests fail without it.
```toml
# ❌ Missing api-version (incorrect)
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
# query_params = { api-version = "2025-04-01-preview" }  # Required
```

- **Don’t mismatch wire_api and endpoint form:** The path must match the API style.
```toml
# ❌ responses API with a /deployments/... path (incorrect)
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai/deployments/YOUR_DEPLOYMENT_NAME"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
wire_api = "responses"
```
```toml
# ❌ chat API without a /deployments/... path (incorrect)
[model_providers.azure]
name = "Azure"
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
env_key = "AZURE_OPENAI_API_KEY"
query_params = { api-version = "2025-04-01-preview" }
wire_api = "chat"
```

- **Don’t bury long comments at line ends:** Avoid trailing, unwrapped comments that cause horizontal scrolling.
```toml
# ❌ Inline, too long (hard to read in rendered Markdown)
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai" # If using chat, this should be https://.../openai/deployments/YOUR_DEPLOYMENT_NAME; otherwise keep as .../openai for responses; copy exactly from Azure AI Foundry since it varies by region.

# ✅ Wrapped above the setting (readable)
# For chat API, include /deployments/YOUR_DEPLOYMENT_NAME.
# For responses API, omit /deployments and keep /openai.
# Copy the exact endpoint from Azure AI Foundry.
base_url = "https://YOUR_PROJECT_NAME.openai.azure.com/openai"
```

- **Don’t guess endpoints:** Copy the exact Azure AI Foundry endpoint and derive `base_url` and `api-version` from it rather than inventing or generalizing.