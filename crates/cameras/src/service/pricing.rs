//! VLM pricing table.
//!
//! Operators want one number on their fleet dashboard: "what is
//! this costing me?" We answer it by multiplying each
//! `compliance_reports.tokens_used` value by a per-model blended
//! rate from this table.
//!
//! ## Why blended, and the trade-off
//!
//! The Anthropic API charges different rates for input vs output
//! tokens. Vision queries with short JSON output are heavy on
//! input (the image gets re-encoded to ~1k tokens per Mpx) and
//! light on output (~100 tokens of JSON). Our worker stores the
//! **sum** in `tokens_used`, not the split, so we can't compute
//! the exact cost from existing rows. The blended rate assumes
//! the 80/20 input/output ratio typical for image + JSON
//! compliance checks; expect ~5% error vs. the actual bill.
//!
//! When we want exact numbers, the worker grows two columns
//! (`input_tokens`, `output_tokens`) and this module switches to
//! per-side rates. The pricing table below already lists both
//! sides so that future swap is a code change here, not a schema
//! migration.
//!
//! ## Keeping prices current
//!
//! Rates come from <https://docs.anthropic.com/en/docs/about-claude/models/overview>
//! and similar provider docs. Bump the constants here when
//! pricing changes; the next `cargo build` picks them up. No
//! database state to migrate.

use chrono::{DateTime, Utc};

/// Price points are denominated in micros (1 USD = 1_000_000
/// micros) per million tokens. Stored as i64 so a year of
/// 500k-token-per-hour traffic still fits without overflow.
#[derive(Debug, Clone, Copy)]
pub struct Price {
    /// Exact model name as sent by the worker, e.g.
    /// `claude-haiku-4-5-20251001`. Matched verbatim against
    /// `compliance_reports.vlm_model`.
    pub model: &'static str,
    /// RFC-3339 timestamp from which this rate applies. Future
    /// price lookups respect this so a rate cut doesn't
    /// retroactively rewrite historical totals.
    pub effective_from: &'static str,
    pub input_per_million_micros: i64,
    pub output_per_million_micros: i64,
    /// Assumed share of `tokens_used` that is input (the rest is
    /// output). Used by [`cost_micros_for`] until the worker
    /// starts splitting the field. Vision + short-JSON queries
    /// average ~0.8.
    pub input_fraction_assumed: f64,
}

/// Returns the published list of prices. Sorted oldest-to-newest
/// per model so [`price_for`] can linear-scan to the latest one
/// that applies at a given timestamp.
pub fn published_prices() -> &'static [Price] {
    &[
        // Claude Haiku 4.5 — current default in
        // video-poc/docker-compose.yml (VLM_MODEL env var).
        // Pricing per Anthropic docs as of May 2026.
        Price {
            model: "claude-haiku-4-5-20251001",
            effective_from: "2025-10-01T00:00:00Z",
            input_per_million_micros: 1_000_000, // $1.00 per million input tokens
            output_per_million_micros: 5_000_000, // $5.00 per million output tokens
            input_fraction_assumed: 0.80,
        },
        // Older Haiku 3.5 for boxes that haven't been bumped yet.
        Price {
            model: "claude-3-5-haiku-20241022",
            effective_from: "2024-10-22T00:00:00Z",
            input_per_million_micros: 800_000,    // $0.80 / M input
            output_per_million_micros: 4_000_000, // $4.00 / M output
            input_fraction_assumed: 0.80,
        },
        // Sonnet 4.6 for the rare operator who picks a stronger
        // model. Pricing per Anthropic May 2026.
        Price {
            model: "claude-sonnet-4-6",
            effective_from: "2026-03-01T00:00:00Z",
            input_per_million_micros: 3_000_000,   // $3 / M input
            output_per_million_micros: 15_000_000, // $15 / M output
            input_fraction_assumed: 0.80,
        },
    ]
}

/// Look up the price that applies for `model` at `ts`. Returns
/// `None` for unknown models — the caller renders those rows as
/// "n/a" rather than zero to avoid masking the issue.
pub fn price_for(model: &str, ts: DateTime<Utc>) -> Option<&'static Price> {
    published_prices()
        .iter()
        .filter(|p| p.model == model)
        .filter(|p| {
            DateTime::parse_from_rfc3339(p.effective_from)
                .map(|t| ts >= t.with_timezone(&Utc))
                .unwrap_or(false)
        })
        .next_back()
}

/// Cost in USD micros for `tokens` worth of `model` calls at `ts`.
/// Returns `None` for unknown models — see [`price_for`].
///
/// V1 uses a blended rate weighted by
/// `input_fraction_assumed`. When the worker starts splitting
/// `tokens_used` into `input_tokens` / `output_tokens`, swap
/// callers to [`cost_micros_for_split`] and this function can
/// go away.
pub fn cost_micros_for(model: &str, tokens: i64, ts: DateTime<Utc>) -> Option<i64> {
    let p = price_for(model, ts)?;
    // Round-to-nearest, not truncate — `as i64` floors, which on the
    // Haiku 4.5 blend produces 1_799_999 instead of 1_800_000 and
    // shows up as "$1.80 minus a micro" in the dashboard.
    let blended_per_million: i64 = ((p.input_per_million_micros as f64) * p.input_fraction_assumed
        + (p.output_per_million_micros as f64) * (1.0 - p.input_fraction_assumed))
        .round() as i64;
    // Round to nearest micro to avoid truncation noise on small
    // token counts. Saturating add to defend against pathological
    // inputs.
    Some(
        (tokens as i128 * blended_per_million as i128 / 1_000_000_i128).clamp(0, i64::MAX as i128)
            as i64,
    )
}

/// Future variant for when worker sends input + output split. Not
/// wired yet; kept here so the call-site change later is trivial.
#[allow(dead_code)]
pub fn cost_micros_for_split(
    model: &str,
    input_tokens: i64,
    output_tokens: i64,
    ts: DateTime<Utc>,
) -> Option<i64> {
    let p = price_for(model, ts)?;
    let input_cost = (input_tokens as i128 * p.input_per_million_micros as i128) / 1_000_000_i128;
    let output_cost =
        (output_tokens as i128 * p.output_per_million_micros as i128) / 1_000_000_i128;
    Some((input_cost + output_cost).clamp(0, i64::MAX as i128) as i64)
}

/// Human-friendly conversion for log + UI: USD micros → string
/// like `"$0.42"` (2 decimal places). Sub-cent values round to
/// `"$0.00"` which is the operator-friendly read for "negligible."
pub fn format_micros_as_usd(micros: i64) -> String {
    format!("${:.2}", micros as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn price_for_returns_latest_applicable() {
        // Haiku 4.5 effective 2025-10-01. A query in 2026 should
        // hit it; a query in 2025-09 should miss it (no rate
        // defined for that model before that date).
        assert!(price_for("claude-haiku-4-5-20251001", ts("2026-05-01T00:00:00Z")).is_some());
        assert!(price_for("claude-haiku-4-5-20251001", ts("2025-09-01T00:00:00Z")).is_none());
    }

    #[test]
    fn cost_micros_for_haiku_4_5_blended_rate() {
        // Blended rate for Haiku 4.5 with 80/20 input/output:
        // 0.8 * 1_000_000 + 0.2 * 5_000_000 = 1_800_000 micros per million tokens.
        // → 1_000_000 tokens = 1_800_000 micros = $1.80.
        let cost = cost_micros_for(
            "claude-haiku-4-5-20251001",
            1_000_000,
            ts("2026-05-01T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(cost, 1_800_000);
        assert_eq!(format_micros_as_usd(cost), "$1.80");
    }

    #[test]
    fn cost_micros_for_unknown_model_is_none() {
        assert!(cost_micros_for("made-up-model", 100, ts("2026-05-01T00:00:00Z")).is_none());
    }

    #[test]
    fn cost_micros_for_zero_tokens_is_zero() {
        let cost =
            cost_micros_for("claude-haiku-4-5-20251001", 0, ts("2026-05-01T00:00:00Z")).unwrap();
        assert_eq!(cost, 0);
    }

    #[test]
    fn format_micros_as_usd_handles_sub_cent() {
        assert_eq!(format_micros_as_usd(0), "$0.00");
        assert_eq!(format_micros_as_usd(999), "$0.00"); // sub-cent rounds down
        assert_eq!(format_micros_as_usd(1_500_000), "$1.50");
    }

    #[test]
    fn cost_micros_for_split_matches_anthropic_billing() {
        // Pure-input 1M tokens on Haiku 4.5 = $1.00.
        let input_only = cost_micros_for_split(
            "claude-haiku-4-5-20251001",
            1_000_000,
            0,
            ts("2026-05-01T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(input_only, 1_000_000);

        // Pure-output 1M tokens = $5.00.
        let output_only = cost_micros_for_split(
            "claude-haiku-4-5-20251001",
            0,
            1_000_000,
            ts("2026-05-01T00:00:00Z"),
        )
        .unwrap();
        assert_eq!(output_only, 5_000_000);
    }
}
