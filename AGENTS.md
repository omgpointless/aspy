# Repository Guidelines

## Project Structure & Module Organization
- `src/main.rs` boots the proxy, wiring config, event tracking, storage, and the TUI loop.
- `src/config.rs` and `src/events.rs` hold configuration, event types, and stats helpers; `src/proxy` runs the Axum server and request interceptor stub; `src/parser` handles Anthropic payload parsing and models; `src/storage` writes JSONL logs under `logs/`; `src/tui` manages the terminal app state and rendering.
- Logs rotate by day to `logs/anthropic-spy-YYYY-MM-DD.jsonl` (gitignored). Binary target is `anthropic-spy` from `cargo build`.

## Build, Test, and Development Commands
- `cargo build --release` – optimized build for local use or packaging.
- `cargo run --release` – start the proxy + TUI; honors env vars like `ANTHROPIC_SPY_BIND`.
- `cargo test` – run unit/integration tests.
- `cargo fmt` / `cargo clippy` / `cargo check` – format, lint, and fast type-check before pushing.
- For verbose diagnostics: `RUST_LOG=debug cargo run`.

## Coding Style & Naming Conventions
- Rust 2021; format with `cargo fmt` (4-space indents, no trailing whitespace).
- Modules/files use `snake_case`; types/traits `PascalCase`; functions/vars `snake_case`; constants `SCREAMING_SNAKE_CASE`.
- Prefer `anyhow::Result` for fallible flows and `thiserror` for domain errors; instrument async tasks with `tracing` spans where useful.
- Keep TUI rendering functions pure; side effects (I/O, logging) stay in proxy/storage layers.

## Testing Guidelines
- Place tests in `#[cfg(test)]` modules near the code; use `tokio::test` for async paths.
- Name tests by behavior (`handles_invalid_payload`, `renders_empty_state`).
- For log-writing code, target a temp dir to avoid polluting `logs/`.
- Run `cargo test` before PRs; add parsing fixtures for new Anthropic payload shapes.

## Commit & Pull Request Guidelines
- Commits: concise, imperative subject lines (e.g., `Add parser validation`); keep scope narrow; reference issues in the body when applicable.
- PRs: include a short summary, test evidence (`cargo test`, `cargo clippy`), and screenshots/GIFs for TUI changes. Call out new env vars or breaking config changes in the description.

## Security & Configuration Tips
- Proxy binds to `127.0.0.1:8080` by default; override with `ANTHROPIC_SPY_BIND`. Ensure `HTTPS_PROXY` points to the chosen port when using Claude Code.
- Do not commit logs or secrets. Validate `ANTHROPIC_API_URL` when pointing to non-default endpoints. Use `ANTHROPIC_SPY_NO_TUI=1` for headless logging in CI or remote environments.
