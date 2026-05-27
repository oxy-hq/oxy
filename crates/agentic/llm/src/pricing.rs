//! Per-model token pricing for run-cost summaries on the coordinator
//! dashboard.
//!
//! Rates are **input-per-million / output-per-million** USD as published
//! by the providers. Cache writes are billed at a markup above input;
//! cache reads at a steep discount. The table is intentionally small —
//! unknown models surface as `cost = None` on the run detail rather than
//! a fabricated number, so a model addition is visible (cost vanishes
//! from the UI) until someone updates this file.
//!
//! Sources: Anthropic & OpenAI public pricing pages, as of the run that
//! `cost_for_model` is called against. If pricing changes after a run
//! completes, the historical cost we display will be slightly off — an
//! acceptable trade for not persisting per-call cost at write time.

/// Per-million-token rates for one model.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    /// Cache *writes* (Anthropic only today). Falls back to input rate
    /// when unset so cache_creation tokens never silently cost nothing.
    pub cache_creation_per_million: Option<f64>,
    /// Cache *reads*. Falls back to a 10× discount on input when unset.
    pub cache_read_per_million: Option<f64>,
}

impl ModelPricing {
    pub const fn new(input: f64, output: f64) -> Self {
        Self {
            input_per_million: input,
            output_per_million: output,
            cache_creation_per_million: None,
            cache_read_per_million: None,
        }
    }

    pub const fn with_cache(mut self, creation: f64, read: f64) -> Self {
        self.cache_creation_per_million = Some(creation);
        self.cache_read_per_million = Some(read);
        self
    }
}

/// Resolve the published rates for `model`. The lookup is a series of
/// prefix matches so model variants (e.g. `claude-sonnet-4-6-20251022`)
/// hit the same rate as the canonical id. Returns `None` for any model
/// not in the table — the caller surfaces this as "cost unavailable".
pub fn rates_for(model: &str) -> Option<ModelPricing> {
    // Anthropic — Claude 4 family.
    if model.starts_with("claude-opus-4") {
        return Some(ModelPricing::new(15.0, 75.0).with_cache(18.75, 1.50));
    }
    if model.starts_with("claude-sonnet-4") {
        return Some(ModelPricing::new(3.0, 15.0).with_cache(3.75, 0.30));
    }
    if model.starts_with("claude-haiku-4") {
        return Some(ModelPricing::new(0.80, 4.0).with_cache(1.0, 0.08));
    }

    // OpenAI — current generation.
    if model.starts_with("gpt-4o-mini") {
        return Some(ModelPricing::new(0.15, 0.60));
    }
    if model.starts_with("gpt-4o") {
        return Some(ModelPricing::new(2.50, 10.0));
    }
    if model.starts_with("o1-mini") {
        return Some(ModelPricing::new(1.10, 4.40));
    }
    if model.starts_with("o1") {
        return Some(ModelPricing::new(15.0, 60.0));
    }

    None
}

/// Compute the USD cost of one call given its token counts. Returns
/// `None` only when the model isn't in the pricing table — partial
/// information (e.g. zero cache tokens) is fine.
pub fn cost_for_call(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> Option<f64> {
    let p = rates_for(model)?;
    // Fallbacks mirror the docstring on `ModelPricing`: cache writes
    // default to the input rate, cache reads to a 10× input discount.
    let cache_creation_rate = p.cache_creation_per_million.unwrap_or(p.input_per_million);
    let cache_read_rate = p
        .cache_read_per_million
        .unwrap_or(p.input_per_million / 10.0);
    let cost = (input_tokens as f64) * p.input_per_million / 1_000_000.0
        + (output_tokens as f64) * p.output_per_million / 1_000_000.0
        + (cache_creation_tokens as f64) * cache_creation_rate / 1_000_000.0
        + (cache_read_tokens as f64) * cache_read_rate / 1_000_000.0;
    Some(cost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_basic_cost() {
        // 1M input + 1M output on Sonnet 4 = $3 + $15 = $18.
        let cost = cost_for_call("claude-sonnet-4-6", 1_000_000, 1_000_000, 0, 0).unwrap();
        assert!((cost - 18.0).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn sonnet_with_cache() {
        // 1M cache read = 1M * $0.30/M = $0.30.
        let cost = cost_for_call("claude-sonnet-4-6", 0, 0, 0, 1_000_000).unwrap();
        assert!((cost - 0.30).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(cost_for_call("gpt-99-future", 1, 1, 0, 0).is_none());
    }

    #[test]
    fn variant_suffix_matches_base() {
        let canon = cost_for_call("claude-sonnet-4-6", 1000, 1000, 0, 0);
        let variant = cost_for_call("claude-sonnet-4-6-20251022", 1000, 1000, 0, 0);
        assert_eq!(canon, variant);
    }
}
