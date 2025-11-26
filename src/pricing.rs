// Pricing calculations for Anthropic API usage
//
// This module provides cost estimation based on official Anthropic pricing.
// Pricing data sourced from: https://www.anthropic.com/pricing
// Last updated: 2025-11-24

/// Pricing information for a specific model
#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
}

/// Get pricing for a specific model
/// Returns default (Sonnet) pricing for unknown models
pub fn get_pricing(model: &str) -> ModelPricing {
    match model {
        // Claude 3.5 Sonnet (Latest)
        "claude-3-5-sonnet-20241022" => ModelPricing {
            input_per_million: 3.00,
            output_per_million: 15.00,
            cache_write_per_million: 3.75,
            cache_read_per_million: 0.30,
        },

        // Claude 3.5 Haiku
        "claude-3-5-haiku-20241022" => ModelPricing {
            input_per_million: 1.00,
            output_per_million: 5.00,
            cache_write_per_million: 1.25,
            cache_read_per_million: 0.10,
        },

        // Claude 3 Opus
        "claude-3-opus-20240229" => ModelPricing {
            input_per_million: 15.00,
            output_per_million: 75.00,
            cache_write_per_million: 18.75,
            cache_read_per_million: 1.50,
        },

        // Claude 3 Sonnet (older)
        "claude-3-sonnet-20240229" => ModelPricing {
            input_per_million: 3.00,
            output_per_million: 15.00,
            cache_write_per_million: 3.75,
            cache_read_per_million: 0.30,
        },

        // Claude 3 Haiku (older)
        "claude-3-haiku-20240307" => ModelPricing {
            input_per_million: 0.25,
            output_per_million: 1.25,
            cache_write_per_million: 0.30,
            cache_read_per_million: 0.03,
        },

        // Default to Claude 3.5 Sonnet pricing for unknown models
        _ => ModelPricing {
            input_per_million: 3.00,
            output_per_million: 15.00,
            cache_write_per_million: 3.75,
            cache_read_per_million: 0.30,
        },
    }
}

/// Calculate cost in USD for the given token usage
pub fn calculate_cost(
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_tokens: u32,
    cache_read_tokens: u32,
) -> f64 {
    let pricing = get_pricing(model);

    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
    let cache_write_cost =
        (cache_creation_tokens as f64 / 1_000_000.0) * pricing.cache_write_per_million;
    let cache_read_cost = (cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_million;

    input_cost + output_cost + cache_write_cost + cache_read_cost
}

/// Calculate how much was saved by using cache reads vs regular input
pub fn calculate_cache_savings(model: &str, cache_read_tokens: u32) -> f64 {
    let pricing = get_pricing(model);

    // Cost if these tokens were regular input
    let regular_cost = (cache_read_tokens as f64 / 1_000_000.0) * pricing.input_per_million;

    // Actual cost with cache read
    let cache_cost = (cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_million;

    // Savings = difference
    regular_cost - cache_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sonnet_pricing() {
        let pricing = get_pricing("claude-3-5-sonnet-20241022");
        assert_eq!(pricing.input_per_million, 3.00);
        assert_eq!(pricing.output_per_million, 15.00);
    }

    #[test]
    fn test_calculate_cost() {
        // Example from ANTHROPIC_PRICING.md
        // Input: 1,000 tokens, Output: 500 tokens
        let cost = calculate_cost("claude-3-5-sonnet-20241022", 1000, 500, 0, 0);
        assert!((cost - 0.0105).abs() < 0.0001); // $0.0105
    }

    #[test]
    fn test_cache_savings() {
        // 10,000 cache read tokens
        let savings = calculate_cache_savings("claude-3-5-sonnet-20241022", 10_000);
        // Regular: 10k * $3.00/1M = $0.03
        // Cache: 10k * $0.30/1M = $0.003
        // Savings: $0.027
        assert!((savings - 0.027).abs() < 0.0001);
    }
}
