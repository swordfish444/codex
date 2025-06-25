# AGENTS.md

This file provides guidance to OpenAI Codex (openai.com/codex) when working with
code in this repository.

## Build, Lint & Test

### JavaScript/TypeScript
- Install dependencies: `pnpm install`
- Run all tests: `pnpm test`
- Run a single test: `pnpm test -- -t <pattern>` or `pnpm test -- path/to/file.spec.ts`
- Watch tests: `pnpm test:watch`
- Lint: `pnpm lint && pnpm lint:fix`
- Type-check: `pnpm typecheck`
- Format: `pnpm format:fix`
- Build: `pnpm build`

### Rust (codex-rs workspace)
- Build: `cargo build --workspace --locked`
- Test all: `cargo test --workspace`
- Test crate: `cargo test -p <crate>`
- Single test: `cargo test -p <crate> -- <test_name>`
- Format & check: `cargo fmt --all -- --check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`

## Code Style Guidelines

- JS/TS: ESLint + Prettier; group imports; camelCase vars & funcs; PascalCase types/components; catch specific errors
- Rust: rustfmt & Clippy (see `codex-rs/rustfmt.toml`); snake_case vars & funcs; PascalCase types; prefer early return; avoid `unwrap()` in prod
- General: Do not swallow exceptions; use DRY; generate/validate ASCII art programmatically
- Include any Cursor rules from `.cursor/rules/` or Copilot rules from `.github/copilot-instructions.md` if present
