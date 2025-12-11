//! Config serialization to TOML
//!
//! Single source of truth for config file format.

use super::{ApiFormat, Config, CountTokensHandling};

impl Config {
    /// Serialize clients HashMap to TOML sections
    pub(super) fn clients_to_toml(&self) -> String {
        if self.clients.clients.is_empty() {
            // Show example comments when no clients configured
            return r#"
# [clients.dev-1]
# name = "Dev Laptop"
# provider = "anthropic"       # References [providers.anthropic] below
"#
            .to_string();
        }

        let mut output = String::from("\n");
        // Sort keys for deterministic output
        let mut keys: Vec<_> = self.clients.clients.keys().collect();
        keys.sort();

        for client_id in keys {
            let client = &self.clients.clients[client_id];
            output.push_str(&format!("[clients.{}]\n", client_id));
            output.push_str(&format!("name = \"{}\"\n", client.name));
            output.push_str(&format!("provider = \"{}\"\n", client.provider));
            if !client.tags.is_empty() {
                output.push_str(&format!("tags = {:?}\n", client.tags));
            }
            output.push('\n');
        }
        output
    }

    /// Serialize providers HashMap to TOML sections
    pub(super) fn providers_to_toml(&self) -> String {
        if self.clients.providers.is_empty() {
            // Show example comments when no providers configured
            return r#"
# [providers.anthropic]
# base_url = "https://api.anthropic.com"
# # count_tokens defaults to "passthrough" for anthropic api_format
#
# # Provider with OpenAI-compatible API (e.g., OpenRouter)
# [providers.openrouter]
# base_url = "https://openrouter.ai/api/v1"
# api_format = "openai"  # Translate Anthropic <-> OpenAI format
# api_path = "/chat/completions"  # Optional: custom path (default: /v1/chat/completions)
# # count_tokens defaults to "synthetic" for openai api_format (endpoint doesn't exist)
# # count_tokens = "dedupe"  # Override: use rate-limited deduplication
# [providers.openrouter.auth]
# method = "bearer"
# key_env = "OPENROUTER_API_KEY"
# strip_incoming = true
# # Per-provider model mapping (overrides global [translation.model_mapping])
# [providers.openrouter.model_mapping]
# "haiku" = "anthropic/claude-3-haiku"
# "sonnet" = "anthropic/claude-sonnet-4"
#
# # Provider with non-standard path (e.g., z.ai)
# [providers.zai]
# base_url = "https://api.z.ai/api/coding/paas/v4"
# api_format = "openai"
# api_path = "/chat/completions"  # Path appended to base_url (no /v1 prefix)
# [providers.zai.model_mapping]
# "haiku" = "grok-3-mini-beta"
# "sonnet" = "grok-3-beta"
"#
            .to_string();
        }

        let mut output = String::from("\n");
        // Sort keys for deterministic output
        let mut keys: Vec<_> = self.clients.providers.keys().collect();
        keys.sort();

        for provider_id in keys {
            let provider = &self.clients.providers[provider_id];
            output.push_str(&format!("[providers.{}]\n", provider_id));
            output.push_str(&format!("base_url = \"{}\"\n", provider.base_url));
            if let Some(name) = &provider.name {
                output.push_str(&format!("name = \"{}\"\n", name));
            }

            // Serialize api_format if not default (anthropic)
            if provider.api_format != ApiFormat::Anthropic {
                output.push_str(&format!(
                    "api_format = \"{}\"\n",
                    provider.api_format.as_str()
                ));
            }

            // Serialize custom api_path if set
            if let Some(api_path) = &provider.api_path {
                output.push_str(&format!("api_path = \"{}\"\n", api_path));
            }

            // Serialize count_tokens if explicitly set AND differs from api_format default
            if let Some(ref ct) = provider.count_tokens {
                let default_for_format = match provider.api_format {
                    ApiFormat::Anthropic => CountTokensHandling::Passthrough,
                    ApiFormat::Openai => CountTokensHandling::Synthetic,
                };
                if ct != &default_for_format {
                    output.push_str(&format!("count_tokens = \"{}\"\n", ct.as_str()));
                }
            }

            // Serialize auth config if present
            if let Some(auth) = &provider.auth {
                output.push_str(&format!("\n[providers.{}.auth]\n", provider_id));
                output.push_str(&format!("method = \"{}\"\n", auth.method.as_str()));
                if let Some(key_env) = &auth.key_env {
                    output.push_str(&format!("key_env = \"{}\"\n", key_env));
                }
                if let Some(key) = &auth.key {
                    output.push_str(&format!("key = \"{}\"\n", key));
                }
                if let Some(header_name) = &auth.header_name {
                    output.push_str(&format!("header_name = \"{}\"\n", header_name));
                }
                if let Some(strip) = auth.strip_incoming {
                    output.push_str(&format!("strip_incoming = {}\n", strip));
                }
            }

            // Serialize model_mapping if non-empty
            if !provider.model_mapping.is_empty() {
                output.push_str(&format!("\n[providers.{}.model_mapping]\n", provider_id));
                // Sort keys for deterministic output
                let mut mapping_keys: Vec<_> = provider.model_mapping.keys().collect();
                mapping_keys.sort();
                for key in mapping_keys {
                    let value = &provider.model_mapping[key];
                    output.push_str(&format!("\"{}\" = \"{}\"\n", key, value));
                }
            }

            output.push('\n');
        }
        output
    }

    /// Serialize transformers config to TOML (returns empty string if not configured)
    pub(super) fn transformers_to_toml(&self) -> String {
        use crate::proxy::transformation::{PositionConfig, RuleConfig};

        let mut output = String::new();

        // Serialize tag-editor if configured
        if let Some(ref editor) = self.transformers.tag_editor {
            if editor.enabled && !editor.rules.is_empty() {
                output.push_str("\n[transformers.tag-editor]\nenabled = true\n");

                for rule in &editor.rules {
                    output.push_str("\n[[transformers.tag-editor.rules]]\n");
                    match rule {
                        RuleConfig::Inject {
                            tag,
                            content,
                            position,
                            when,
                        } => {
                            output.push_str("type = \"inject\"\n");
                            output.push_str(&format!("tag = \"{}\"\n", tag));
                            // Escape content for TOML multiline if needed
                            if content.contains('\n') {
                                output
                                    .push_str(&format!("content = \"\"\"\n{}\n\"\"\"\n", content));
                            } else {
                                output.push_str(&format!("content = \"{}\"\n", content));
                            }
                            match position {
                                PositionConfig::Start => {
                                    output.push_str("position = \"start\"\n");
                                }
                                PositionConfig::End => {
                                    // end is default, can omit
                                }
                                PositionConfig::Before { pattern } => {
                                    output.push_str(&format!(
                                        "position.before.pattern = \"{}\"\n",
                                        pattern
                                    ));
                                }
                                PositionConfig::After { pattern } => {
                                    output.push_str(&format!(
                                        "position.after.pattern = \"{}\"\n",
                                        pattern
                                    ));
                                }
                            }
                            // Output when condition using dotted keys (valid TOML for array elements)
                            if let Some(cond) = when {
                                cond.write_toml(&mut output);
                            }
                        }
                        RuleConfig::Remove { tag, pattern, when } => {
                            output.push_str("type = \"remove\"\n");
                            output.push_str(&format!("tag = \"{}\"\n", tag));
                            output.push_str(&format!("pattern = \"{}\"\n", pattern));
                            if let Some(cond) = when {
                                cond.write_toml(&mut output);
                            }
                        }
                        RuleConfig::Replace {
                            tag,
                            pattern,
                            replacement,
                            when,
                        } => {
                            output.push_str("type = \"replace\"\n");
                            output.push_str(&format!("tag = \"{}\"\n", tag));
                            output.push_str(&format!("pattern = \"{}\"\n", pattern));
                            output.push_str(&format!("replacement = \"{}\"\n", replacement));
                            if let Some(cond) = when {
                                cond.write_toml(&mut output);
                            }
                        }
                    }
                }
            }
        }

        // Serialize system-editor if configured
        if let Some(ref editor) = self.transformers.system_editor {
            if editor.enabled && !editor.rules.is_empty() {
                output.push_str(
                    r#"
# ─────────────────────────────────────────────────────────────────────────────
# SYSTEM EDITOR
# ─────────────────────────────────────────────────────────────────────────────
# Modifies system prompts in API requests.

[transformers.system-editor]
enabled = true
"#,
                );

                for rule in &editor.rules {
                    output.push_str("\n[[transformers.system-editor.rules]]\n");
                    match rule {
                        crate::proxy::transformation::system_editor::RuleConfig::Append {
                            content,
                        } => {
                            output.push_str("type = \"append\"\n");
                            if content.contains('\n') {
                                output
                                    .push_str(&format!("content = \"\"\"\n{}\n\"\"\"\n", content));
                            } else {
                                output.push_str(&format!("content = \"{}\"\n", content));
                            }
                        }
                        crate::proxy::transformation::system_editor::RuleConfig::Prepend {
                            content,
                        } => {
                            output.push_str("type = \"prepend\"\n");
                            if content.contains('\n') {
                                output
                                    .push_str(&format!("content = \"\"\"\n{}\n\"\"\"\n", content));
                            } else {
                                output.push_str(&format!("content = \"{}\"\n", content));
                            }
                        }
                        crate::proxy::transformation::system_editor::RuleConfig::Replace {
                            pattern,
                            replacement,
                        } => {
                            output.push_str("type = \"replace\"\n");
                            output.push_str(&format!("pattern = \"{}\"\n", pattern));
                            output.push_str(&format!("replacement = \"{}\"\n", replacement));
                        }
                    }
                }
            }
        }

        // Serialize compact-enhancer if configured
        if let Some(ref compact) = self.transformers.compact_enhancer {
            if compact.enabled {
                output.push_str(
                    r#"
# ─────────────────────────────────────────────────────────────────────────────
# COMPACTION ENHANCER
# ─────────────────────────────────────────────────────────────────────────────
# Detects Anthropic's compaction prompt and injects continuity guidance.
# The summarizing Claude writes a handoff note; the continuing Claude reads it.

[transformers.compact-enhancer]
enabled = true
"#,
                );
            }
        }

        output
    }

    /// Serialize config to TOML string (single source of truth for format)
    pub fn to_toml(&self) -> String {
        format!(
            r#"# aspy configuration

# Theme: Spy Dark, Spy Light, One Half Dark, Dracula, Nord, Gruvbox Dark, Monokai Pro, etc.
# See full list in the theme selector (press 't' in the TUI)
theme = "{theme}"

# Use theme's background color (true) or terminal's default (false)
use_theme_background = {use_bg}

# Layout preset: classic, reasoning, debug
preset = "{preset}"

# Context window limit for the gauge
context_limit = {limit}

# Proxy bind address
bind_addr = "{bind}"

# Log directory for session files
log_dir = "{log_dir}"

# Feature flags
[features]
storage = {storage}
thinking_panel = {thinking}
stats = {stats}

# Augmentation (response modifications)
[augmentation]
context_warning = {ctx_warn}
context_warning_thresholds = {thresholds:?}

# Logging configuration (RUST_LOG env var overrides)
[logging]
level = "{log_level}"
# File logging (in addition to TUI buffer or stdout)
file_enabled = {log_file_enabled}
file_dir = "{log_file_dir}"
file_rotation = "{log_file_rotation}"  # hourly, daily, never
file_prefix = "{log_file_prefix}"

# Lifetime statistics storage (SQLite-backed context recovery)
[cortex]
enabled = {cortex_enabled}
db_path = "{cortex_db_path}"
store_thinking = {cortex_store_thinking}
store_tool_io = {cortex_store_tool_io}
max_thinking_size = {cortex_max_thinking_size}
retention_days = {cortex_retention_days}
channel_buffer = {cortex_channel_buffer}
batch_size = {cortex_batch_size}
flush_interval_secs = {cortex_flush_interval_secs}

# ─────────────────────────────────────────────────────────────────────────────
# SEMANTIC SEARCH EMBEDDINGS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Enable vector embeddings for semantic search alongside FTS5 keyword search.
# API keys should be set via environment variables (OPENAI_API_KEY, etc.)
#
# Provider options: "none" (default), "local", "remote"
# - none: FTS5 keyword search only (no embeddings)
# - local: ONNX models via fastembed (requires --features local-embeddings)
# - remote: OpenAI-compatible API (OpenAI, Azure, OpenRouter)
[embeddings]
provider = "{embed_provider}"
model = "{embed_model}"
{embed_api_base}{embed_auth_method}poll_interval_secs = {embed_poll_interval}
batch_size = {embed_batch_size}
batch_delay_ms = {embed_batch_delay}
max_content_length = {embed_max_content}

# ─────────────────────────────────────────────────────────────────────────────
# API TRANSLATION (Optional - OpenAI ↔ Anthropic)
# ─────────────────────────────────────────────────────────────────────────────
# Enable bidirectional translation between OpenAI and Anthropic API formats.
# When enabled, the proxy can accept OpenAI-formatted requests (/v1/chat/completions),
# translate them to Anthropic format, and translate responses back.
#
# Use case: Run OpenAI-compatible tools through Anthropic's API.

[translation]
enabled = {translation_enabled}
auto_detect = {translation_auto_detect}
{translation_model_mapping}
# ─────────────────────────────────────────────────────────────────────────────
# REQUEST TRANSFORMERS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Modify API requests before they are forwarded to the provider.
# Use for editing <system-reminder> tags, injecting context, etc.
#
# IMPORTANT: Set enabled = true to activate transformers.

[transformers]
enabled = {transformers_enabled}

# System Reminder Editor - modify <system-reminder> tags in user messages
# Rules are applied in order. Rule types:
#   inject  - Add new <system-reminder> content (position: start, end, before, after)
#   remove  - Remove reminders matching a regex pattern
#   replace - Replace content within matching reminders
#
# Example: Inject a custom context reminder
# [transformers.tag-editor]
# enabled = true
# [[transformers.tag-editor.rules]]
# type = "inject"
# content = "Always respond in formal English."
# position = "end"
#
# Example: Remove noisy debug reminders
# [[transformers.tag-editor.rules]]
# type = "remove"
# pattern = "debug|noisy"  # Regex: removes reminders containing "debug" or "noisy"
#
# Example: Replace text within reminders (supports capture groups)
# [[transformers.tag-editor.rules]]
# type = "replace"
# pattern = "version (\\d+\\.\\d+)"  # Capture the version number
# replacement = "version $1-patched"  # Use $1, $2, etc. for captured groups
#
# System Editor - modify the system prompt (CLAUDE.md, etc.)
# Rule types: append, prepend, replace
#
# Example: Append instructions to the system prompt
# [transformers.system-editor]
# enabled = true
# [[transformers.system-editor.rules]]
# type = "append"
# content = "Always prioritize security best practices."
#
# Example: Replace text in the system prompt
# [[transformers.system-editor.rules]]
# type = "replace"
# pattern = "old text"
# replacement = "new text"
#
# Compaction Enhancer - inject continuity guidance when Claude Code runs /compact
# [transformers.compact-enhancer]
# enabled = true
{transformers_section}
# ─────────────────────────────────────────────────────────────────────────────
# OPENTELEMETRY EXPORT (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Export telemetry to Azure Application Insights or other OTel-compatible backends.
# Connection string can also be set via APPLICATIONINSIGHTS_CONNECTION_STRING env var.

[otel]
enabled = {otel_enabled}
{otel_connection_string}service_name = "{otel_service_name}"
service_version = "{otel_service_version}"

# ─────────────────────────────────────────────────────────────────────────────
# MULTI-CLIENT ROUTING (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Track multiple Claude Code instances through a single proxy using named clients.
# Each client connects via URL path: http://localhost:8080/<client-id>
#
# Example: ANTHROPIC_BASE_URL=http://127.0.0.1:8080/dev-1 claude
{clients_section}
# ─────────────────────────────────────────────────────────────────────────────
# PROVIDER BACKENDS (Optional)
# ─────────────────────────────────────────────────────────────────────────────
# Define where to forward API requests. Clients reference these by name.
{providers_section}
"#,
            theme = self.theme,
            use_bg = self.use_theme_background,
            preset = self.preset,
            limit = self.context_limit,
            bind = self.bind_addr,
            log_dir = self.log_dir.display(),
            storage = self.features.json_logging,
            thinking = self.features.thinking_panel,
            stats = self.features.stats,
            ctx_warn = self.augmentation.context_warning,
            thresholds = self.augmentation.context_warning_thresholds,
            log_level = self.logging.level,
            log_file_enabled = self.logging.file_enabled,
            log_file_dir = self.logging.file_dir.display(),
            log_file_rotation = self.logging.file_rotation.as_str(),
            log_file_prefix = self.logging.file_prefix,
            cortex_enabled = self.cortex.enabled,
            cortex_db_path = self.cortex.db_path.display(),
            cortex_store_thinking = self.cortex.store_thinking,
            cortex_store_tool_io = self.cortex.store_tool_io,
            cortex_max_thinking_size = self.cortex.max_thinking_size,
            cortex_retention_days = self.cortex.retention_days,
            cortex_channel_buffer = self.cortex.channel_buffer,
            cortex_batch_size = self.cortex.batch_size,
            cortex_flush_interval_secs = self.cortex.flush_interval_secs,
            translation_enabled = self.translation.enabled,
            translation_auto_detect = self.translation.auto_detect,
            translation_model_mapping = if self.translation.model_mapping.is_empty() {
                r#"
# Model mappings (source model -> target model)
# Uncomment and customize as needed. Built-in defaults handle common models.
# [translation.model_mapping]
# "gpt-4" = "claude-sonnet-4-20250514"
# "gpt-3.5-turbo" = "claude-3-haiku-20240307"
"#
                .to_string()
            } else {
                let mut mappings = String::from("\n[translation.model_mapping]\n");
                let mut keys: Vec<_> = self.translation.model_mapping.keys().collect();
                keys.sort();
                for key in keys {
                    let value = &self.translation.model_mapping[key];
                    mappings.push_str(&format!("\"{}\" = \"{}\"\n", key, value));
                }
                mappings
            },
            embed_provider = self.embeddings.provider,
            embed_model = self.embeddings.model,
            embed_api_base = self
                .embeddings
                .api_base
                .as_ref()
                .map(|url| format!("api_base = \"{}\"\n", url))
                .unwrap_or_else(|| {
                    "# api_base = \"https://api.openai.com/v1\"  # For remote provider\n"
                        .to_string()
                }),
            embed_auth_method = if self.embeddings.auth_method != "bearer" {
                format!("auth_method = \"{}\"\n", self.embeddings.auth_method)
            } else {
                "# auth_method = \"bearer\"  # \"bearer\" or \"api-key\" (Azure)\n".to_string()
            },
            embed_poll_interval = self.embeddings.poll_interval_secs,
            embed_batch_size = self.embeddings.batch_size,
            embed_batch_delay = self.embeddings.batch_delay_ms,
            embed_max_content = self.embeddings.max_content_length,
            transformers_enabled = self.transformers.enabled,
            transformers_section = self.transformers_to_toml(),
            otel_enabled = self.otel.enabled,
            otel_connection_string = self
                .otel
                .connection_string
                .as_ref()
                .map(|cs| format!("connection_string = \"{}\"\n", cs))
                .unwrap_or_else(|| {
                    "# connection_string = \"InstrumentationKey=...;IngestionEndpoint=...\"\n"
                        .to_string()
                }),
            otel_service_name = self.otel.service_name,
            otel_service_version = self.otel.service_version,
            clients_section = self.clients_to_toml(),
            providers_section = self.providers_to_toml(),
        )
    }

    /// Save current configuration to file
    pub fn save(&self) -> Result<(), std::io::Error> {
        let Some(path) = Self::config_path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Could not determine config path",
            ));
        };

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&path, self.to_toml())
    }
}
