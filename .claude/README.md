# .claude/ Directory Structure

This directory contains project documentation, reference materials, and Claude Code configuration for the anthropic-spy project.

## Purpose

The `.claude/` directory serves dual purposes:

1. **Version-Controlled Knowledge** - Stable architectural docs, vision, API references, and default commands
2. **Local Workspace** - Session-specific notes, temporal context, and personal learning scratchpads (ignored by git)

## What Gets Committed

These files are tracked in version control as they benefit all contributors:

### Core Documentation
- `EXTENSIONS_VISION.md` - Future extensibility roadmap (HTTP API, MCP, hooks)
- `PROJECT_STATUS.md` - Current development phase and roadmap
- `impl-plans/` - RFC-style implementation plans for upcoming features

### Deprecated (Moved to docs/)
The following have been superseded by documentation in `docs/`:
- `ARCHITECTURE.md` - **Deprecated**: See `docs/architecture.md`
- `DEVELOPMENT_PHILOSOPHY.md` - **Deprecated**: See `docs/architecture.md` and `AGENTS.md`
- `TONE_GUIDE.md` - **Deprecated**: See `AGENTS.md`

**Note:** The root `CLAUDE.md` is the primary project documentation. Use `docs/` for detailed reference docs.

### Claude Code Plugin
- `.claude-plugin/` - Optional development tooling (committed)
  - Provides `/aspy:stats` and `/aspy:tempcontext` slash commands
  - Automatic `cargo fmt` hooks for Rust files
  - Install with: `/plugin marketplace add /path/to/anthropic-spy`

### Meta
- `README.md` - This file
- `.gitignore` - Controls what gets committed vs. ignored

## What Gets Ignored

These files are kept local as they're session-specific or personal workspace material:

### Session Context
- `SESSION_*.md` - Session-specific notes and handoffs
- `temporal-context/` - Timestamped session logs (e.g., `2025-11-30-theme-system.md`)
- `snapshots/` - Session state snapshots

### Exploratory Work
- `roadmap-brainstorms/` - Early-stage ideation and planning
- `*LEARNING*.md` - Personal learning notes (Rust, Docker, etc.)
- `*PRICING*.md` - Pricing references that change over time

### Temporal Planning
- `RELEASE_PLAN*.md` - Version-specific release planning docs
- `QUICK_REFERENCE.md` - Session-specific quick references
- `START_HERE.md` - Session entry points

### User Customizations (Future)
- `hooks/` - User-provided hook scripts (see EXTENSIONS_VISION.md Phase 4)
- `*.local.md` - Local overrides to documentation
- `.state/`, `.cache/` - Runtime state

## Directory Layout

```
.claude/
├── README.md                    # This file (committed)
├── .gitignore                   # Controls commit/ignore (committed)
│
├── CLAUDE.md                    # Main instructions (committed)
├── EXTENSIONS_VISION.md         # Extensibility roadmap (committed)
├── ARCHITECTURE.md              # System design (committed)
├── DEVELOPMENT_PHILOSOPHY.md    # Rust patterns (committed)
├── PROJECT_STATUS.md            # Current phase (committed)
├── TONE_GUIDE.md                # Messaging guide (committed)
│
├── SESSION_*.md                 # Session notes (ignored)
├── temporal-context/            # Session logs (ignored)
├── snapshots/                   # Session snapshots (ignored)
├── roadmap-brainstorms/         # Exploratory notes (ignored)
└── [other ignored patterns]     # See .gitignore
```

## For Contributors

### Adding Stable Documentation

If you're adding architectural docs, vision statements, or reference materials that benefit all contributors:

1. Create the file in `.claude/` root or appropriate subdirectory
2. Ensure it's NOT matched by `.claude/.gitignore` patterns
3. Commit with: `git add .claude/<filename>`
4. Include in PR for review

### Working with Session Context

For session-specific notes, temporal context, or personal learning:

1. Use naming patterns that match `.claude/.gitignore` (e.g., `SESSION_*.md`)
2. Or place in ignored directories (`temporal-context/`, `snapshots/`, etc.)
3. These files stay local, never commit them

### Installing Slash Commands

Slash commands in `commands/spy/*.md` are examples. To use them:

1. Symlink or copy to your local `.claude/commands/` if using Claude Code
2. Or reference them when creating custom commands
3. See EXTENSIONS_VISION.md for the full vision

## For Users

### Finding Documentation

- **"How should I architect this?"** → `ARCHITECTURE.md`, `DEVELOPMENT_PHILOSOPHY.md`
- **"What's the vision for extensions?"** → `EXTENSIONS_VISION.md`
- **"What Rust patterns do we use?"** → `DEVELOPMENT_PHILOSOPHY.md`
- **"Where are we in development?"** → `PROJECT_STATUS.md`

### Session Notes

Feel free to create session-specific notes using these patterns (auto-ignored):

- `SESSION_<topic>.md` - Handoff notes
- `temporal-context/YYYY-MM-DD-<topic>.md` - Timestamped context
- `snapshots/YYYY-MM-DD-<description>.md` - State snapshots
- `*LEARNING*.md` - Personal learning notes

These won't pollute the repo, but stay in your local checkout.

## Alignment with EXTENSIONS_VISION.md

This structure supports the extensions vision:

- **Plugin system** in `.claude-plugin/` provides opt-in tooling (slash commands + hooks)
- **Architectural docs** ensure extensions follow composition patterns
- **User customizations** in `.claude/hooks/` stay local (ignored by git)

See `EXTENSIONS_VISION.md` for the full roadmap of HTTP API, MCP server, and hook integration.

## Questions?

- Check `.claude/.gitignore` for the full list of ignore patterns
- See `CLAUDE.md` for main project instructions
- Review `EXTENSIONS_VISION.md` for future directions
