# Multi-Client Routing & Session Tracking

anthropic-spy supports tracking multiple Claude Code instances through a single proxy using **named client routing**. Each client is identified by a configured ID and routed to its designated provider backend.

## Client Routing Model

**How it works:**
1. Define clients and providers in your config file
2. Each client connects via a unique URL path: `http://localhost:8080/<client-id>`
3. The proxy extracts the client ID from the URL and routes to the configured provider
4. Each client gets isolated session tracking and statistics

**Example:** A client configured as `dev-1` connects via `http://localhost:8080/dev-1` and is routed to their configured provider (Anthropic, Foundry, AWS Bedrock, GCP Vertex, etc.)

## Configuration

Configuration lives at `~/.config/anthropic-spy/config.toml`.

### Defining Clients

```toml
[clients.dev-1]
name = "Dev Laptop"
provider = "anthropic"
tags = ["dev", "primary"]

[clients.ci]
name = "CI Runner"
provider = "foundry"

[clients.team-alice]
name = "Alice's Workstation"
provider = "anthropic"
tags = ["team"]
```

**Client fields:**
| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Human-readable display name |
| `provider` | Yes | Provider key (must match a `[providers.*]` section) |
| `tags` | No | Optional tags for grouping/filtering |

### Defining Providers

```toml
[providers.anthropic]
base_url = "https://api.anthropic.com"
name = "Anthropic Direct"

[providers.foundry]
base_url = "https://your-instance.services.ai.azure.com/anthropic"
name = "Azure AI Foundry"

[providers.bedrock]
base_url = "https://bedrock-runtime.us-east-1.amazonaws.com"
name = "AWS Bedrock"
```

**Provider fields:**
| Field | Required | Description |
|-------|----------|-------------|
| `base_url` | Yes | The upstream API URL to forward requests to |
| `name` | No | Human-readable display name |

## Connecting Claude Code

Set the `ANTHROPIC_BASE_URL` environment variable to include your client ID:

```powershell
# Windows PowerShell
$env:ANTHROPIC_BASE_URL="http://127.0.0.1:8080/dev-1"
claude
```

```bash
# macOS/Linux
export ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1
claude
```

The proxy reads the path segment (`dev-1`), looks up the client configuration, and routes all requests to that client's provider.

## Use Cases

### Same API Key, Multiple Sessions

**Problem:** You have one Anthropic API key but want to track separate Claude Code sessions (e.g., different projects, different terminals).

**Solution:** Create multiple client entries pointing to the same provider:

```toml
[clients.project-a]
name = "Project A"
provider = "anthropic"

[clients.project-b]
name = "Project B"
provider = "anthropic"

[providers.anthropic]
base_url = "https://api.anthropic.com"
name = "Anthropic Direct"
```

Now start two Claude instances with different client IDs:
- Terminal 1: `ANTHROPIC_BASE_URL=http://127.0.0.1:8080/project-a`
- Terminal 2: `ANTHROPIC_BASE_URL=http://127.0.0.1:8080/project-b`

Each session is tracked independently in the TUI and logs.

### Multi-Provider Routing

**Problem:** You have access to Claude through different providers (direct Anthropic, Azure Foundry, AWS Bedrock) and want to switch between them.

**Solution:** Configure multiple providers and assign them to different clients:

```toml
[clients.direct]
name = "Anthropic Direct"
provider = "anthropic"

[clients.work]
name = "Work (Foundry)"
provider = "foundry"

[providers.anthropic]
base_url = "https://api.anthropic.com"
name = "Anthropic"

[providers.foundry]
base_url = "https://company.services.ai.azure.com/anthropic"
name = "Azure AI Foundry"
```

### Team Observability

**Problem:** Multiple team members want to use a shared anthropic-spy instance for visibility.

**Solution:** Create a client entry for each team member:

```toml
[clients.alice]
name = "Alice"
provider = "anthropic"
tags = ["team", "frontend"]

[clients.bob]
name = "Bob"
provider = "anthropic"
tags = ["team", "backend"]
```

## Session Tracking

### What's Tracked Per Client

- Client ID and display name
- Provider being used
- Session start time and status (active/idle/ended)
- Per-session statistics (requests, tokens, costs, tool calls)
- Per-session event buffer (last 500 events)

### Session Lifecycle

```
Claude Code connects → Client ID extracted from URL path
    ↓
Client config lookup → Provider determined
    ↓
Requests proxied → Events recorded to client's session
    ↓
Session ends → Archived with full statistics
```

## API Endpoints

All endpoints support optional `?client=<id>` filtering:

| Endpoint | Without filter | With `?client=<id>` |
|----------|----------------|---------------------|
| `GET /api/stats` | Global aggregate stats | Client's session stats |
| `GET /api/events` | Global event buffer | Client's session events |
| `GET /api/context` | Global context status | Client's context status |
| `GET /api/sessions` | All sessions | All sessions (no filter) |
| `GET /api/clients` | Configured clients | N/A |

**Examples:**
```bash
# See all configured clients
curl http://127.0.0.1:8080/api/clients

# Get stats for a specific client
curl "http://127.0.0.1:8080/api/stats?client=dev-1"

# See all active sessions
curl http://127.0.0.1:8080/api/sessions
```

## MCP Integration

The `aspy` MCP server can scope queries to a specific client:

```typescript
// MCP tools can auto-detect client from environment or accept explicit client ID
const endpoint = `/api/stats?client=${clientId}`;
```

**Available MCP Tools:**
- `aspy_stats` - Session statistics (scoped to client)
- `aspy_events` - Session events (scoped to client)
- `aspy_context` - Context window status (scoped to client)
- `aspy_sessions` - All sessions with `is_me` flag

## Future: Proxy Tokens

For environments where you want authentication without exposing API keys, a future enhancement will support **proxy tokens**:

```toml
[clients.dev-1]
name = "Dev Laptop"
provider = "anthropic"
proxy_token = "secret-token-abc123"  # Future feature
```

The client would then authenticate to the proxy using the token rather than passing through the upstream API key. This keeps API keys centralized on the proxy server.

## Migration from API Key Hashing

If you previously used the API key hash-based tracking:

**Old approach:**
- Identity derived from SHA-256 hash of API key
- All requests to `http://127.0.0.1:8080` auto-detected by key

**New approach:**
- Identity derived from URL path segment
- Explicit client configuration in `config.toml`
- Connect to `http://127.0.0.1:8080/<client-id>`

**Benefits:**
- Same API key can track multiple sessions
- Explicit naming instead of cryptic hashes
- Provider routing per-client
- Easier team setup
