//! Human-readable formatting for prices, money, and labels.

use serde_json::Value as JsonValue;

pub(in crate::service) fn format_price_label(
    nickname: &Option<String>,
    amount_display: &str,
    interval: &str,
) -> String {
    let body = format!("{amount_display} / {interval}");
    match nickname {
        Some(n) if !n.is_empty() => format!("{n} — {body}"),
        _ => body,
    }
}

pub(in crate::service) fn format_money(amount: i64, currency: &str) -> String {
    let value = amount as f64 / 100.0;
    let cur = currency.to_uppercase();
    format!("${value:.2} {cur}")
}

/// Build a human-readable amount string. For tiered prices Stripe leaves the
/// top-level `unit_amount` null, so fall back to the first tier's
/// `unit_amount` + `flat_amount`. Multi-tier prices show the first tier with
/// a `Tiered (from …)` prefix so admins can still tell prices apart.
pub(in crate::service) fn format_amount_display(
    price: &JsonValue,
    unit_amount: i64,
    currency: &str,
) -> String {
    let billing_scheme = price["billing_scheme"].as_str().unwrap_or("per_unit");
    if billing_scheme == "tiered" {
        if let Some(tiers) = price["tiers"].as_array()
            && let Some(first) = tiers.first()
        {
            let unit = first["unit_amount"].as_i64().unwrap_or(0);
            let flat = first["flat_amount"].as_i64().unwrap_or(0);
            let mut parts: Vec<String> = Vec::with_capacity(2);
            if unit > 0 {
                parts.push(format!("{}/unit", format_money(unit, currency)));
            }
            if flat > 0 {
                parts.push(format_money(flat, currency));
            }
            if !parts.is_empty() {
                let joined = parts.join(" + ");
                if tiers.len() > 1 {
                    return format!("Tiered (from {joined})");
                }
                return joined;
            }
        }
        return "Tiered pricing".to_string();
    }
    format_money(unit_amount, currency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- format_money ----

    #[test]
    fn format_money_uppercases_currency_and_uses_dollar_sign() {
        // Note: the `$` prefix is hard-coded; this is a known limitation of
        // the v1 admin UI (USD-only). Locking it in so a future change is
        // intentional.
        assert_eq!(format_money(1234, "usd"), "$12.34 USD");
        assert_eq!(format_money(0, "eur"), "$0.00 EUR");
    }

    // ---- format_price_label ----

    #[test]
    fn format_price_label_includes_nickname_when_present() {
        let nick = Some("Pro Plan".into());
        assert_eq!(
            format_price_label(&nick, "$10.00 USD", "month"),
            "Pro Plan — $10.00 USD / month"
        );
    }

    #[test]
    fn format_price_label_omits_empty_nickname() {
        assert_eq!(
            format_price_label(&None, "$10.00 USD", "month"),
            "$10.00 USD / month"
        );
        assert_eq!(
            format_price_label(&Some(String::new()), "$10.00 USD", "month"),
            "$10.00 USD / month"
        );
    }

    // ---- format_amount_display: per_unit ----

    #[test]
    fn amount_display_per_unit_uses_unit_amount() {
        let p = json!({"billing_scheme": "per_unit"});
        assert_eq!(format_amount_display(&p, 2500, "usd"), "$25.00 USD");
    }

    #[test]
    fn amount_display_missing_billing_scheme_treated_as_per_unit() {
        let p = json!({});
        assert_eq!(format_amount_display(&p, 100, "usd"), "$1.00 USD");
    }

    // ---- format_amount_display: tiered ----

    #[test]
    fn amount_display_tiered_single_tier_with_unit_amount_only() {
        let p = json!({
            "billing_scheme": "tiered",
            "tiers": [{"unit_amount": 500, "flat_amount": 0}],
        });
        assert_eq!(format_amount_display(&p, 0, "usd"), "$5.00 USD/unit");
    }

    #[test]
    fn amount_display_tiered_single_tier_with_flat_amount_only() {
        let p = json!({
            "billing_scheme": "tiered",
            "tiers": [{"unit_amount": 0, "flat_amount": 5000}],
        });
        assert_eq!(format_amount_display(&p, 0, "usd"), "$50.00 USD");
    }

    #[test]
    fn amount_display_tiered_single_tier_with_both_components() {
        let p = json!({
            "billing_scheme": "tiered",
            "tiers": [{"unit_amount": 500, "flat_amount": 5000}],
        });
        assert_eq!(
            format_amount_display(&p, 0, "usd"),
            "$5.00 USD/unit + $50.00 USD"
        );
    }

    #[test]
    fn amount_display_tiered_multi_tier_prefixes_with_from() {
        // Multi-tier shows the first tier with a "Tiered (from …)" prefix
        // so admins can distinguish prices in the dropdown.
        let p = json!({
            "billing_scheme": "tiered",
            "tiers": [
                {"unit_amount": 500, "flat_amount": 0},
                {"unit_amount": 400, "flat_amount": 0},
            ],
        });
        assert_eq!(
            format_amount_display(&p, 0, "usd"),
            "Tiered (from $5.00 USD/unit)"
        );
    }

    #[test]
    fn amount_display_tiered_with_no_tier_data_falls_back_to_label() {
        // tiers array missing entirely (Stripe didn't expand `tiers` field).
        let p = json!({"billing_scheme": "tiered"});
        assert_eq!(format_amount_display(&p, 0, "usd"), "Tiered pricing");
    }

    #[test]
    fn amount_display_tiered_with_zero_amounts_falls_back_to_label() {
        // Tier exists but both amounts are 0 — neither part is pushed,
        // so the joined parts list is empty.
        let p = json!({
            "billing_scheme": "tiered",
            "tiers": [{"unit_amount": 0, "flat_amount": 0}],
        });
        assert_eq!(format_amount_display(&p, 0, "usd"), "Tiered pricing");
    }
}
