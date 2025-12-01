# Contributing to Anthropic Spy

Thank you for your interest in contributing! This guide will help you get started.

## Development Setup

### Prerequisites

- **Rust 1.75+** - [Install via rustup](https://rustup.rs/)
- **Git** - For version control
- **Terminal with Unicode/color support** - For testing the TUI

### Getting Started

```bash
# Clone the repository
git clone https://github.com/omgpointless/anthropic-spy.git
cd anthropic-spy

# Build and run
cargo build
cargo run

# Run tests
cargo test

# Check formatting and lints
cargo fmt --check
cargo clippy
```

## Workflow (GitHub Flow)

We use [GitHub Flow](https://docs.github.com/en/get-started/quickstart/github-flow), a lightweight branch-based workflow:

1. **Create a branch** from `main` for your work
2. **Make changes** with clear, focused commits
3. **Open a Pull Request** when ready for review
4. **Discuss and iterate** based on feedback
5. **Merge** once approved

### Branch Naming

Use descriptive branch names:
- `feat/thinking-panel-filter` - New features
- `fix/sse-parsing-timeout` - Bug fixes
- `docs/quickstart-improvements` - Documentation
- `refactor/parser-cleanup` - Code refactoring

## Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `perf` | Performance improvement |
| `test` | Adding or correcting tests |
| `build` | Build system or dependencies |
| `ci` | CI configuration |
| `chore` | Other maintenance tasks |
| `revert` | Revert a previous commit |

### Validation

Commit messages are validated at two levels:

1. **Local hook** (optional but recommended):
   ```bash
   git config core.hooksPath .githooks
   ```
   This validates your commits locally before they're created.

2. **CI check** (required):
   All commits in a PR are validated by CI. PRs with non-conforming commits will fail the check.

### Examples

```
feat(tui): add filtering by tool name

fix(parser): handle empty SSE delta events

docs: update quickstart for Windows users

refactor(proxy)!: restructure header extraction

BREAKING CHANGE: HeadersCaptured event format changed
```

## Code Style

### Formatting

Run `cargo fmt` before committing. CI will reject unformatted code.

### Linting

Run `cargo clippy` and address warnings. We aim for zero warnings.

### Guidelines

- **Error handling**: Use `Result<T, E>` with `?` operator and `.context()` for error messages
- **Avoid `.unwrap()`**: Use `.expect("reason")` only where failure is truly impossible
- **Comments**: Explain *why*, not *what* - code should be self-documenting
- **Dependencies**: Justify new dependencies - we keep the dependency tree lean

### Security

- **Never log API keys** - Use hash prefix only (see `extract_request_headers()`)
- **Validate inputs** at system boundaries
- **Be mindful of sensitive data** in tool call payloads

## Pull Request Process

### Before Opening a PR

- [ ] Commits follow [Conventional Commits](https://www.conventionalcommits.org/) format
- [ ] Code compiles: `cargo check`
- [ ] Tests pass: `cargo test`
- [ ] Code is formatted: `cargo fmt`
- [ ] No clippy warnings: `cargo clippy`
- [ ] Tested manually with Claude Code (if applicable)

### PR Description

Include:
- **What** the change does
- **Why** it's needed
- **How** to test it
- Screenshots/GIFs for TUI changes

### Review Process

- PRs require at least one approval
- Address feedback by pushing additional commits (don't force-push during review)
- Squash merge is preferred for clean history

## Testing

### Manual Testing

The best way to test is with a live Claude Code session:

```bash
# Terminal 1: Run the proxy
cargo run --release

# Terminal 2: Run Claude Code through the proxy
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080
claude
```

### Demo Mode

For TUI testing without API costs:

```bash
ASPY_DEMO=1 cargo run --release
```

### What to Verify

- TUI renders correctly (no visual glitches)
- Events appear in correct order
- Navigation works (arrow keys, j/k, Enter)
- Status bar updates
- Logs don't break through TUI

## Reporting Issues

### Bug Reports

Include:
- **Expected behavior** vs **actual behavior**
- **Steps to reproduce**
- **Environment** (OS, Rust version, terminal)
- **Logs** (from `./logs/*.jsonl` if relevant)

### Feature Requests

- Describe the **use case** (what problem does it solve?)
- Consider **scope** (does it fit the project's purpose?)
- **Alternatives** you've considered

## Questions?

- Open a [GitHub Discussion](https://github.com/omgpointless/anthropic-spy/discussions)
- Check existing [Issues](https://github.com/omgpointless/anthropic-spy/issues)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
