//! Embedding provider abstraction for semantic search
//!
//! This module provides a trait-based system for generating embeddings from text.
//! Embeddings are used for semantic search (vector similarity) as an opt-in
//! enhancement to FTS5 keyword search.
//!
//! # Architecture
//!
//! ```text
//! EmbeddingProvider trait
//! ├── NoOpProvider (disabled, FTS-only fallback)
//! ├── RemoteProvider (OpenAI, Azure, etc.) - Phase 2
//! └── LocalProvider (fastembed-rs / ONNX) - Phase 2
//! ```
//!
//! # Design Principles
//!
//! 1. **Opt-in**: Semantic search is additive - FTS5 works standalone
//! 2. **Provider abstraction**: Swap between remote APIs and local models
//! 3. **Background indexing**: Don't block the pipeline; async catch-up
//! 4. **Query-time embedding**: Embed search queries inline (acceptable latency)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Standard embedding dimensions for common models
pub mod dimensions {
    /// MiniLM-L6-v2 (all-MiniLM-L6-v2) - fast, small, good quality
    pub const MINILM_L6: usize = 384;

    /// MiniLM-L12-v2 - better quality, slightly slower
    pub const MINILM_L12: usize = 384;

    /// BGE-small-en-v1.5 - good for English text
    pub const BGE_SMALL: usize = 384;

    /// BGE-base-en-v1.5 - better quality
    pub const BGE_BASE: usize = 768;

    /// OpenAI text-embedding-3-small
    pub const OPENAI_SMALL: usize = 1536;

    /// OpenAI text-embedding-3-large
    pub const OPENAI_LARGE: usize = 3072;

    /// OpenAI text-embedding-ada-002 (legacy)
    pub const OPENAI_ADA: usize = 1536;
}

/// Embedding vector type
///
/// Using `Vec<f32>` for flexibility across different dimensions.
/// For 384-dim embeddings, this is ~1.5KB per vector.
pub type Embedding = Vec<f32>;

/// Result of an embedding operation
#[derive(Debug, Clone)]
pub struct EmbeddingResult {
    /// The embedding vector
    pub embedding: Embedding,
    /// Token count (if available from provider)
    pub tokens_used: Option<u32>,
}

/// Batch embedding result
#[derive(Debug, Clone)]
pub struct BatchEmbeddingResult {
    /// Embeddings in the same order as input texts
    pub embeddings: Vec<Embedding>,
    /// Total tokens used across all texts
    pub total_tokens: Option<u32>,
}

/// Error type for embedding operations
/// Errors that can occur during embedding operations
///
/// Some variants are reserved for specific provider implementations:
/// - ModelLoadError: local ONNX providers (requires --features local-embeddings)
/// - TextTooLong: when input exceeds model context window
#[derive(Debug)]
pub enum EmbeddingError {
    /// Provider is not configured or disabled
    NotConfigured,
    /// Rate limit exceeded (includes retry-after hint)
    RateLimited { retry_after_secs: Option<u64> },
    /// API error from remote provider
    ApiError { status: u16, message: String },
    /// Network error
    NetworkError(String),
    /// Model loading error (local provider)
    #[allow(dead_code)] // Used by local-embeddings feature (ONNX model loading)
    ModelLoadError(String),
    /// Text too long for model's context window
    #[allow(dead_code)] // Reserved for chunking/truncation implementation
    TextTooLong { max_tokens: usize },
    /// Internal error
    Internal(String),
}

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "Embedding provider not configured"),
            Self::RateLimited { retry_after_secs } => {
                if let Some(secs) = retry_after_secs {
                    write!(f, "Rate limited, retry after {} seconds", secs)
                } else {
                    write!(f, "Rate limited")
                }
            }
            Self::ApiError { status, message } => {
                write!(f, "API error ({}): {}", status, message)
            }
            Self::NetworkError(msg) => write!(f, "Network error: {}", msg),
            Self::ModelLoadError(msg) => write!(f, "Model load error: {}", msg),
            Self::TextTooLong { max_tokens } => {
                write!(f, "Text too long (max {} tokens)", max_tokens)
            }
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for EmbeddingError {}

/// Provider type for configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// No embeddings (FTS-only mode)
    #[default]
    None,
    /// Local ONNX model (fastembed-rs)
    Local,
    /// Remote OpenAI-compatible API (OpenAI, Azure v1, OpenRouter, etc.)
    Remote,
}

impl fmt::Display for ProviderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Local => write!(f, "local"),
            Self::Remote => write!(f, "remote"),
        }
    }
}

/// Authentication method for remote providers
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    /// Bearer token in Authorization header (OpenAI, OpenRouter)
    #[default]
    Bearer,
    /// API key in api-key header (Azure)
    ApiKey,
}

/// Configuration for embedding providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Which provider to use
    pub provider: ProviderType,

    /// Model name/identifier
    /// - Local: "all-MiniLM-L6-v2", "bge-small-en-v1.5", etc.
    /// - Remote: "text-embedding-3-small", "text-embedding-3-large"
    pub model: String,

    /// API key (for remote providers)
    /// Can also be set via environment variables (OPENAI_API_KEY, etc.)
    #[serde(skip_serializing)] // Don't serialize API keys
    pub api_key: Option<String>,

    /// API base URL for remote providers
    /// - OpenAI: "https://api.openai.com/v1" (default)
    /// - Azure v1: "https://{resource}.openai.azure.com/openai/v1"
    /// - OpenRouter: "https://openrouter.ai/api/v1"
    pub api_base: Option<String>,

    /// API version query parameter (for Azure AI Foundry)
    /// - Azure: "preview" or specific version like "2024-10-21"
    pub api_version: Option<String>,

    /// Authentication method for remote providers
    /// - Bearer: Authorization: Bearer {key} (OpenAI, OpenRouter)
    /// - ApiKey: api-key: {key} (Azure)
    #[serde(default)]
    pub auth_method: AuthMethod,

    /// Embedding dimensions (auto-detected from model if not specified)
    pub dimensions: Option<usize>,

    /// Maximum batch size for embedding requests
    pub batch_size: usize,

    /// Request timeout in seconds
    pub timeout_secs: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: ProviderType::None,
            model: String::new(),
            api_key: None,
            api_base: None,
            api_version: None,
            auth_method: AuthMethod::Bearer,
            dimensions: None,
            batch_size: 32,
            timeout_secs: 30,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Convenience constructors for programmatic config
// Currently config is loaded from TOML (config.rs), but these are useful for:
// - Tests
// - Future CLI quick-setup commands (e.g., `aspy embeddings --setup openai`)
// - Library usage if extracted as a crate
// ─────────────────────────────────────────────────────────────────────────────
#[allow(dead_code)]
impl EmbeddingConfig {
    /// Create config for local MiniLM model
    pub fn local_minilm() -> Self {
        Self {
            provider: ProviderType::Local,
            model: "all-MiniLM-L6-v2".to_string(),
            dimensions: Some(dimensions::MINILM_L6),
            ..Default::default()
        }
    }

    /// Create config for OpenAI embeddings
    pub fn openai(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let dimensions = Self::infer_dimensions(&model);

        Self {
            provider: ProviderType::Remote,
            model,
            api_key: Some(api_key.into()),
            api_base: Some("https://api.openai.com/v1".to_string()),
            auth_method: AuthMethod::Bearer,
            dimensions,
            ..Default::default()
        }
    }

    /// Create config for Azure AI Foundry v1 API
    pub fn azure(resource: &str, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let dimensions = Self::infer_dimensions(&model);

        Self {
            provider: ProviderType::Remote,
            model,
            api_key: Some(api_key.into()),
            api_base: Some(format!("https://{}.openai.azure.com/openai/v1", resource)),
            auth_method: AuthMethod::ApiKey,
            dimensions,
            ..Default::default()
        }
    }

    /// Create config for OpenRouter
    pub fn openrouter(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        let model = model.into();
        let dimensions = Self::infer_dimensions(&model);

        Self {
            provider: ProviderType::Remote,
            model,
            api_key: Some(api_key.into()),
            api_base: Some("https://openrouter.ai/api/v1".to_string()),
            auth_method: AuthMethod::Bearer,
            dimensions,
            ..Default::default()
        }
    }

    /// Create config for any OpenAI-compatible endpoint
    pub fn custom(
        api_base: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
        auth_method: AuthMethod,
    ) -> Self {
        let model = model.into();
        let dimensions = Self::infer_dimensions(&model);

        Self {
            provider: ProviderType::Remote,
            model,
            api_key: Some(api_key.into()),
            api_base: Some(api_base.into()),
            auth_method,
            dimensions,
            ..Default::default()
        }
    }

    /// Check if embeddings are enabled
    pub fn is_enabled(&self) -> bool {
        self.provider != ProviderType::None
    }

    /// Get the embedding dimensions for this config
    pub fn get_dimensions(&self) -> usize {
        self.dimensions
            .unwrap_or_else(|| Self::infer_dimensions(&self.model).unwrap_or(dimensions::MINILM_L6))
    }

    /// Infer dimensions from model name
    fn infer_dimensions(model: &str) -> Option<usize> {
        match model {
            m if m.contains("MiniLM-L6") => Some(dimensions::MINILM_L6),
            m if m.contains("MiniLM-L12") => Some(dimensions::MINILM_L12),
            m if m.contains("bge-small") => Some(dimensions::BGE_SMALL),
            m if m.contains("bge-base") => Some(dimensions::BGE_BASE),
            m if m.contains("embedding-3-small") => Some(dimensions::OPENAI_SMALL),
            m if m.contains("embedding-3-large") => Some(dimensions::OPENAI_LARGE),
            m if m.contains("ada-002") => Some(dimensions::OPENAI_ADA),
            _ => None,
        }
    }

    /// Get the effective API base URL
    pub fn get_api_base(&self) -> &str {
        self.api_base
            .as_deref()
            .unwrap_or("https://api.openai.com/v1")
    }
}

/// Trait for embedding providers
///
/// # Sync Design
///
/// Methods are synchronous to match the event pipeline design. Providers
/// that need async I/O should use internal channels or block_on.
///
/// # Thread Safety
///
/// Providers must be `Send + Sync` for use across threads.
///
/// # Note on unused methods
///
/// Some trait methods (name, dimensions, shutdown) are not called in current
/// code paths but are part of the provider contract for:
/// - Logging and debugging (name)
/// - Validation and status reporting (dimensions)
/// - Resource cleanup (shutdown)
#[allow(unused)]
pub trait EmbeddingProvider: Send + Sync {
    /// Human-readable name for logging
    fn name(&self) -> &'static str;

    /// Get the embedding dimensions for this provider
    fn dimensions(&self) -> usize;

    /// Check if the provider is ready to generate embeddings
    fn is_ready(&self) -> bool;

    /// Generate an embedding for a single text
    ///
    /// # Arguments
    /// * `text` - The text to embed
    ///
    /// # Returns
    /// The embedding vector or an error
    fn embed(&self, text: &str) -> Result<EmbeddingResult, EmbeddingError>;

    /// Generate embeddings for multiple texts in a batch
    ///
    /// Default implementation calls `embed()` for each text.
    /// Providers should override this for efficient batching.
    ///
    /// # Arguments
    /// * `texts` - The texts to embed
    ///
    /// # Returns
    /// Embeddings in the same order as input texts
    fn embed_batch(&self, texts: &[&str]) -> Result<BatchEmbeddingResult, EmbeddingError> {
        let mut embeddings = Vec::with_capacity(texts.len());
        let mut total_tokens = 0u32;

        for text in texts {
            let result = self.embed(text)?;
            embeddings.push(result.embedding);
            if let Some(tokens) = result.tokens_used {
                total_tokens += tokens;
            }
        }

        Ok(BatchEmbeddingResult {
            embeddings,
            total_tokens: if total_tokens > 0 {
                Some(total_tokens)
            } else {
                None
            },
        })
    }

    /// Shutdown the provider (cleanup resources)
    fn shutdown(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

/// No-op embedding provider (FTS-only mode)
///
/// Used when semantic search is disabled. All operations return
/// `EmbeddingError::NotConfigured`.
#[derive(Debug, Default)]
pub struct NoOpProvider;

impl NoOpProvider {
    pub fn new() -> Self {
        Self
    }
}

impl EmbeddingProvider for NoOpProvider {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn is_ready(&self) -> bool {
        false
    }

    fn embed(&self, _text: &str) -> Result<EmbeddingResult, EmbeddingError> {
        Err(EmbeddingError::NotConfigured)
    }

    fn embed_batch(&self, _texts: &[&str]) -> Result<BatchEmbeddingResult, EmbeddingError> {
        Err(EmbeddingError::NotConfigured)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OpenAI-Compatible Remote Provider
// ═══════════════════════════════════════════════════════════════════════════

/// Remote embedding provider using OpenAI-compatible API
///
/// Works with:
/// - OpenAI (https://api.openai.com/v1)
/// - Azure AI Foundry v1 (https://{resource}.openai.azure.com/openai/v1)
/// - OpenRouter (https://openrouter.ai/api/v1)
/// - Any OpenAI-compatible endpoint
pub struct OpenAiCompatibleProvider {
    client: reqwest::blocking::Client,
    base_url: String,
    api_key: String,
    auth_method: AuthMethod,
    model: String,
    api_version: Option<String>,
    #[allow(dead_code)] // Used by dimensions() trait method, reserved for status reporting
    dimensions: usize,
}

impl OpenAiCompatibleProvider {
    /// Create a new OpenAI-compatible embedding provider
    ///
    /// # Arguments
    /// * `config` - Embedding configuration
    ///
    /// # Errors
    /// Returns an error if API key is missing or client creation fails
    pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbeddingError> {
        let api_key = config
            .api_key
            .clone()
            .ok_or(EmbeddingError::NotConfigured)?;

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| {
                EmbeddingError::NetworkError(format!("Failed to create HTTP client: {}", e))
            })?;

        let base_url = config.get_api_base().to_string();
        let dimensions = config.get_dimensions();

        tracing::info!(
            "Initialized OpenAI-compatible provider: {} (model: {}, {} dims, auth: {:?})",
            base_url,
            config.model,
            dimensions,
            config.auth_method
        );

        Ok(Self {
            client,
            base_url,
            api_key,
            auth_method: config.auth_method.clone(),
            model: config.model.clone(),
            api_version: config.api_version.clone(),
            dimensions,
        })
    }

    /// Build the embeddings request
    fn build_request(&self, input: &[&str]) -> reqwest::blocking::RequestBuilder {
        // Build URL with optional api-version query parameter (required for Azure AI Foundry)
        let base = format!("{}/embeddings", self.base_url.trim_end_matches('/'));
        let url = match &self.api_version {
            Some(version) => format!("{}?api-version={}", base, version),
            None => base,
        };

        let mut req = self.client.post(&url);

        // Set auth header based on method
        req = match &self.auth_method {
            AuthMethod::Bearer => req.header("Authorization", format!("Bearer {}", self.api_key)),
            AuthMethod::ApiKey => req.header("api-key", &self.api_key),
        };

        req.header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "model": self.model,
                "input": input,
            }))
    }

    /// Parse the embeddings response
    fn parse_response(
        &self,
        response: reqwest::blocking::Response,
    ) -> Result<BatchEmbeddingResult, EmbeddingError> {
        let status = response.status();

        if !status.is_success() {
            let status_code = status.as_u16();

            // Try to parse error body
            let error_text = response
                .text()
                .unwrap_or_else(|_| "Unknown error".to_string());

            // Check for rate limiting
            if status_code == 429 {
                return Err(EmbeddingError::RateLimited {
                    retry_after_secs: None,
                });
            }

            return Err(EmbeddingError::ApiError {
                status: status_code,
                message: error_text,
            });
        }

        // Parse successful response
        let body: OpenAiEmbeddingResponse = response
            .json()
            .map_err(|e| EmbeddingError::Internal(format!("Failed to parse response: {}", e)))?;

        // Sort by index and extract embeddings
        let mut data = body.data;
        data.sort_by_key(|d| d.index);

        let embeddings = data.into_iter().map(|d| d.embedding).collect();
        let total_tokens = body.usage.map(|u| u.total_tokens);

        Ok(BatchEmbeddingResult {
            embeddings,
            total_tokens,
        })
    }
}

/// OpenAI embeddings API response format
#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: u32,
}

impl EmbeddingProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &'static str {
        "openai-compatible"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn embed(&self, text: &str) -> Result<EmbeddingResult, EmbeddingError> {
        let result = self.embed_batch(&[text])?;

        if result.embeddings.is_empty() {
            return Err(EmbeddingError::Internal(
                "No embedding returned".to_string(),
            ));
        }

        Ok(EmbeddingResult {
            embedding: result.embeddings.into_iter().next().unwrap(),
            tokens_used: result.total_tokens,
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<BatchEmbeddingResult, EmbeddingError> {
        if texts.is_empty() {
            return Ok(BatchEmbeddingResult {
                embeddings: Vec::new(),
                total_tokens: None,
            });
        }

        let request = self.build_request(texts);
        let response = request
            .send()
            .map_err(|e| EmbeddingError::NetworkError(format!("Request failed: {}", e)))?;

        self.parse_response(response)
    }
}

/// Create an embedding provider from configuration
///
/// # Arguments
/// * `config` - The embedding configuration
///
/// # Returns
/// A boxed provider implementing `EmbeddingProvider`
pub fn create_provider(config: &EmbeddingConfig) -> Box<dyn EmbeddingProvider> {
    match config.provider {
        ProviderType::None => Box::new(NoOpProvider::new()),
        ProviderType::Local => {
            #[cfg(feature = "local-embeddings")]
            {
                match LocalProvider::new(config) {
                    Ok(provider) => Box::new(provider),
                    Err(e) => {
                        tracing::error!("Failed to create local embedding provider: {}", e);
                        Box::new(NoOpProvider::new())
                    }
                }
            }
            #[cfg(not(feature = "local-embeddings"))]
            {
                tracing::warn!(
                    "Local embeddings feature not enabled. Build with --features local-embeddings"
                );
                Box::new(NoOpProvider::new())
            }
        }
        ProviderType::Remote => match OpenAiCompatibleProvider::new(config) {
            Ok(provider) => Box::new(provider),
            Err(e) => {
                tracing::error!("Failed to create remote embedding provider: {}", e);
                Box::new(NoOpProvider::new())
            }
        },
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Local Provider (fastembed-rs / ONNX)
// ═══════════════════════════════════════════════════════════════════════════

/// Local embedding provider using ONNX models via fastembed-rs
///
/// Supports the following models:
/// - all-MiniLM-L6-v2 (384 dimensions, default)
/// - all-MiniLM-L12-v2 (384 dimensions)
/// - bge-small-en-v1.5 (384 dimensions)
/// - bge-base-en-v1.5 (768 dimensions)
///
/// Models are downloaded on first use (~20-80MB depending on model).
#[cfg(feature = "local-embeddings")]
pub struct LocalProvider {
    model: fastembed::TextEmbedding,
    model_name: String,
    dimensions: usize,
}

#[cfg(feature = "local-embeddings")]
impl LocalProvider {
    /// Create a new local embedding provider
    ///
    /// # Arguments
    /// * `config` - Embedding configuration specifying the model
    ///
    /// # Errors
    /// Returns an error if the model fails to load
    pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbeddingError> {
        use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

        // Map model name to fastembed enum
        let (model_enum, dimensions) = match config.model.as_str() {
            "all-MiniLM-L6-v2" | "AllMiniLML6V2" | "" => {
                (EmbeddingModel::AllMiniLML6V2, dimensions::MINILM_L6)
            }
            "all-MiniLM-L12-v2" | "AllMiniLML12V2" => {
                (EmbeddingModel::AllMiniLML12V2, dimensions::MINILM_L12)
            }
            "bge-small-en-v1.5" | "BGESmallENV15" => {
                (EmbeddingModel::BGESmallENV15, dimensions::BGE_SMALL)
            }
            "bge-base-en-v1.5" | "BGEBaseENV15" => {
                (EmbeddingModel::BGEBaseENV15, dimensions::BGE_BASE)
            }
            _ => {
                return Err(EmbeddingError::ModelLoadError(format!(
                    "Unknown model: {}. Supported: all-MiniLM-L6-v2, all-MiniLM-L12-v2, bge-small-en-v1.5, bge-base-en-v1.5",
                    config.model
                )));
            }
        };

        tracing::info!(
            "Loading local embedding model: {} ({} dimensions)",
            config.model,
            dimensions
        );

        // Initialize the model
        let model = TextEmbedding::try_new(InitOptions::new(model_enum)).map_err(|e| {
            EmbeddingError::ModelLoadError(format!("Failed to initialize model: {}", e))
        })?;

        tracing::info!("Local embedding model loaded successfully");

        Ok(Self {
            model,
            model_name: config.model.clone(),
            dimensions,
        })
    }
}

#[cfg(feature = "local-embeddings")]
impl EmbeddingProvider for LocalProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn is_ready(&self) -> bool {
        true // Model is loaded at construction
    }

    fn embed(&self, text: &str) -> Result<EmbeddingResult, EmbeddingError> {
        let embeddings = self
            .model
            .embed(vec![text], None)
            .map_err(|e| EmbeddingError::Internal(format!("Embedding failed: {}", e)))?;

        if embeddings.is_empty() {
            return Err(EmbeddingError::Internal(
                "No embedding returned".to_string(),
            ));
        }

        Ok(EmbeddingResult {
            embedding: embeddings.into_iter().next().unwrap(),
            tokens_used: None, // Local model doesn't track tokens
        })
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<BatchEmbeddingResult, EmbeddingError> {
        if texts.is_empty() {
            return Ok(BatchEmbeddingResult {
                embeddings: Vec::new(),
                total_tokens: None,
            });
        }

        let texts_owned: Vec<String> = texts.iter().map(|s| (*s).to_string()).collect();

        let embeddings = self
            .model
            .embed(texts_owned, None)
            .map_err(|e| EmbeddingError::Internal(format!("Batch embedding failed: {}", e)))?;

        Ok(BatchEmbeddingResult {
            embeddings,
            total_tokens: None,
        })
    }
}

/// Embedding indexer status for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStatus {
    /// Provider type
    pub provider: ProviderType,
    /// Model name
    pub model: String,
    /// Embedding dimensions
    pub dimensions: usize,
    /// Whether provider is ready
    pub is_ready: bool,
    /// Total documents indexed
    pub documents_indexed: u64,
    /// Documents pending indexing
    pub documents_pending: u64,
    /// Percentage complete
    pub index_progress_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_provider() {
        let provider = NoOpProvider::new();
        assert_eq!(provider.name(), "noop");
        assert_eq!(provider.dimensions(), 0);
        assert!(!provider.is_ready());

        let result = provider.embed("test");
        assert!(matches!(result, Err(EmbeddingError::NotConfigured)));
    }

    #[test]
    fn test_embedding_config_defaults() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, ProviderType::None);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_embedding_config_local() {
        let config = EmbeddingConfig::local_minilm();
        assert_eq!(config.provider, ProviderType::Local);
        assert_eq!(config.model, "all-MiniLM-L6-v2");
        assert_eq!(config.get_dimensions(), dimensions::MINILM_L6);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_embedding_config_openai() {
        let config = EmbeddingConfig::openai("sk-test", "text-embedding-3-small");
        assert_eq!(config.provider, ProviderType::Remote);
        assert_eq!(config.get_dimensions(), dimensions::OPENAI_SMALL);
        assert!(config.is_enabled());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_dimension_inference() {
        let mut config = EmbeddingConfig::default();

        config.model = "all-MiniLM-L6-v2".to_string();
        assert_eq!(config.get_dimensions(), dimensions::MINILM_L6);

        config.model = "bge-base-en-v1.5".to_string();
        assert_eq!(config.get_dimensions(), dimensions::BGE_BASE);

        config.model = "text-embedding-3-large".to_string();
        assert_eq!(config.get_dimensions(), dimensions::OPENAI_LARGE);
    }

    #[test]
    fn test_provider_type_display() {
        assert_eq!(ProviderType::None.to_string(), "none");
        assert_eq!(ProviderType::Local.to_string(), "local");
        assert_eq!(ProviderType::Remote.to_string(), "remote");
    }
}
