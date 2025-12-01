# Project Hooks

This directory contains Claude Code hooks for the aspy plugin.

## Active Hooks

### `session-start.sh` - Session Registration

**Trigger:** SessionStart (startup, resume, clear, compact)
**Action:** Registers the session with anthropic-spy proxy for tracking
**Timeout:** 10 seconds

When Claude Code starts a session:
1. Hook receives session_id and source from Claude Code
2. Computes user_id by hashing ANTHROPIC_API_KEY (SHA-256, first 16 chars)
3. POSTs to `http://127.0.0.1:8080/api/session/start`
4. Proxy supersedes any previous session for this user

**Configuration:** Set `ASPY_API_URL` env var to override proxy address.

---

### `session-end.sh` - Session Archival

**Trigger:** SessionEnd (clear, logout, prompt_input_exit, other)
**Action:** Notifies proxy that session has ended
**Timeout:** 5 seconds (fire-and-forget)

When Claude Code ends:
1. Hook receives session_id and reason
2. POSTs to `http://127.0.0.1:8080/api/session/end`
3. Proxy archives the session for history

---

### `cargo-fmt.sh` - Automatic Rust Formatting

**Trigger:** After Write or Edit tool calls on `.rs` files
**Action:** Runs `cargo fmt` on the modified file
**Timeout:** 30 seconds

When Claude Code writes or edits a Rust file:
1. The PostToolUse hook fires automatically
2. The hook script receives tool call data via stdin
3. Script extracts the file path and checks if it's a `.rs` file
4. If yes, runs `cargo fmt` on that specific file
5. Formatting errors are logged but don't block (non-fatal)

---

## Multi-User Session Tracking

The session hooks enable multi-user tracking:

```
User A (api_key_hash: a3f2c91b)
├── Session 1: 14:30-15:45 → tracked, archived
└── Session 2: 16:00-...   → active

User B (api_key_hash: 7e1d04fa)
└── Session 1: 15:00-...   → active
```

Query sessions via API:
```bash
curl http://127.0.0.1:8080/api/sessions
```

---

## Testing Hooks

```bash
# Test session-start hook
echo '{"session_id":"test-123","source":"startup"}' | \
  ANTHROPIC_API_KEY="sk-ant-test" ./session-start.sh

# Test session-end hook
echo '{"session_id":"test-123","reason":"logout"}' | \
  ANTHROPIC_API_KEY="sk-ant-test" ./session-end.sh

# Test cargo-fmt hook
echo '{"name":"Write","input":{"file_path":"src/main.rs"}}' | \
  ./cargo-fmt.sh
```

---

## Resources

- [Claude Code Hooks Guide](https://code.claude.com/docs/en/hooks-guide)
- [Hooks Reference](https://code.claude.com/docs/en/hooks)
- [anthropic-spy README](../../README.md)
