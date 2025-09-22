# Agent Harness Fixtures

These fixtures drive the integration tests in `core/tests/suite/agent_harness.rs`.
Each subdirectory under this folder corresponds to a single end-to-end scenario.

## Adding a New Fixture

1. Create a new directory inside `tests/fixtures/harness/` and give it a
   descriptive name (e.g. `multi_tool_call`).
2. Add the following JSON files inside the directory:
   - `user_prompts.json`: the list of `Op` objects that will be submitted to the
     harness.
   - `response_events.json`: the SSE payloads that the mock Responses API will
     replay. A top-level array can contain objects (single request) or arrays
     (multiple sequential requests).
   - `expected_request.json`: the sanitized request body we expect the harness
     to send. This can be either a single object or an array when multiple
     requests are issued.
   - `expected_events.json`: the sanitized Codex events we expect to observe.
3. Run `cargo test -p codex-core suite::agent_harness::<fixture_name>` to verify
   the scenario passes.

The test macro in `core/tests/suite/agent_harness.rs` will automatically pick up
the new directory once it exists.

## Expected JSON Files Are Partial

The comparison helpers only assert that the fields present in the expected JSON
match the actual values. Any keys omitted from `expected_request.json` or
`expected_events.json` are treated as "don't care" and are ignored during the
assertion. This keeps the fixtures stable even when Codex introduces new fields;
only include the pieces that matter for the scenario you are describing.
