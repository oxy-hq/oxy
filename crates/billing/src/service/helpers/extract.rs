//! JSON extraction + Unix-time conversion helpers.

use chrono::{TimeZone, Utc};
use sea_orm::prelude::DateTimeWithTimeZone;
use serde_json::Value as JsonValue;

use crate::errors::BillingError;
use crate::service::stripe_shapes::StripeSubscriptionItem;

/// Resolve which subscription item carries the seat quantity. Tries:
/// 1. cached `stripe_subscription_seat_item_id` from `org_billing`
/// 2. items tagged with `metadata.oxy_role = "seat"` (set at provision time)
/// 3. fallback heuristic: shortest recurring interval (legacy subs)
pub(in crate::service) fn pick_seat_item<'a>(
    cached_id: &Option<String>,
    items: &'a [StripeSubscriptionItem],
) -> Result<&'a StripeSubscriptionItem, BillingError> {
    if let Some(cached) = cached_id {
        if let Some(found) = items.iter().find(|i| i.id == *cached) {
            return Ok(found);
        }
    }
    // Items provisioned by us carry `metadata.oxy_role = "seat"` — set in
    // `provision_subscription` (direct API) and `create_checkout_session`
    // (line_items metadata, which Stripe propagates to subscription items).
    if let Some(tagged) = items
        .iter()
        .find(|i| i.metadata.get("oxy_role").map(String::as_str) == Some("seat"))
    {
        return Ok(tagged);
    }
    // Defense-in-depth fallback for legacy subscriptions provisioned before
    // the metadata tag was introduced, or if Checkout-propagation behavior
    // changes upstream. Pick the shortest interval (seat is monthly/weekly,
    // flat fee is yearly).
    let picked = items
        .iter()
        .min_by_key(
            |i| match i.price.recurring.as_ref().map(|r| r.interval.as_str()) {
                Some("day") => 0,
                Some("week") => 1,
                Some("month") => 2,
                Some("year") => 3,
                _ => 99,
            },
        )
        .ok_or(BillingError::MalformedStripeResponse)?;
    tracing::warn!(
        item_id = %picked.id,
        "pick_seat_item: oxy_role metadata missing, fell back to interval heuristic"
    );
    Ok(picked)
}

pub(in crate::service) fn extract_item_id_for_price(
    subscription: &JsonValue,
    price_id: &str,
) -> Result<String, BillingError> {
    subscription["items"]["data"]
        .as_array()
        .and_then(|items| {
            items.iter().find_map(|item| {
                let item_price_id = item["price"]["id"].as_str()?;
                if item_price_id == price_id {
                    item["id"].as_str().map(str::to_string)
                } else {
                    None
                }
            })
        })
        .ok_or(BillingError::MalformedStripeResponse)
}

pub(in crate::service) fn extract_period(
    subscription: &JsonValue,
) -> (Option<DateTimeWithTimeZone>, Option<DateTimeWithTimeZone>) {
    // Under `billing_mode=flexible` each item carries its own period.
    // Take the earliest period across items so callers see the soonest
    // upcoming renewal.
    let items = subscription["items"]["data"].as_array();
    let min_field = |field: &str| -> Option<i64> {
        items.and_then(|items| items.iter().filter_map(|i| i[field].as_i64()).min())
    };
    let start = min_field("current_period_start").and_then(time_from_unix);
    let end = min_field("current_period_end").and_then(time_from_unix);
    (start, end)
}

pub(in crate::service) fn time_from_unix(secs: i64) -> Option<DateTimeWithTimeZone> {
    Utc.timestamp_opt(secs, 0).single().map(Into::into)
}

/// Extract subscription id from an invoice payload. Stripe API 2024+ moved
/// the field from top-level `invoice.subscription` to nested
/// `invoice.parent.subscription_details.subscription`. Try both for safety.
pub(in crate::service) fn invoice_subscription_id(invoice: &JsonValue) -> Option<String> {
    if let Some(s) = invoice["parent"]["subscription_details"]["subscription"].as_str() {
        return Some(s.to_string());
    }
    invoice["subscription"].as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::stripe_shapes::{StripeRecurring, StripeSubItemPrice};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn sub_item(
        id: &str,
        interval: Option<&str>,
        oxy_role: Option<&str>,
    ) -> StripeSubscriptionItem {
        let mut metadata = BTreeMap::new();
        if let Some(role) = oxy_role {
            metadata.insert("oxy_role".into(), role.into());
        }
        StripeSubscriptionItem {
            id: id.into(),
            price: StripeSubItemPrice {
                id: format!("price_for_{id}"),
                recurring: interval.map(|i| StripeRecurring { interval: i.into() }),
            },
            quantity: Some(1),
            current_period_start: None,
            current_period_end: None,
            metadata,
        }
    }

    // ---- pick_seat_item ----

    #[test]
    fn pick_seat_uses_cached_id_first() {
        let items = vec![
            sub_item("si_seat", Some("month"), Some("seat")),
            sub_item("si_flat", Some("year"), Some("flat")),
        ];
        let cached = Some("si_seat".to_string());
        let picked = pick_seat_item(&cached, &items).unwrap();
        assert_eq!(picked.id, "si_seat");
    }

    #[test]
    fn pick_seat_falls_back_to_metadata_when_cache_misses() {
        // Cached id no longer present (item replaced). Should fall through
        // to the metadata tag rather than the interval heuristic.
        let items = vec![
            sub_item("si_year_no_role", Some("year"), None),
            sub_item("si_month_seat", Some("month"), Some("seat")),
        ];
        let cached = Some("si_gone".to_string());
        let picked = pick_seat_item(&cached, &items).unwrap();
        assert_eq!(picked.id, "si_month_seat");
    }

    #[test]
    fn pick_seat_uses_metadata_when_no_cache() {
        let items = vec![
            sub_item("si_a", Some("month"), None),
            sub_item("si_b", Some("year"), Some("seat")),
        ];
        let picked = pick_seat_item(&None, &items).unwrap();
        assert_eq!(picked.id, "si_b");
    }

    #[test]
    fn pick_seat_falls_back_to_shortest_interval_when_no_metadata() {
        // Legacy subs provisioned before the oxy_role tag was added.
        // Must pick the shortest interval (seat = monthly, flat = yearly).
        let items = vec![
            sub_item("si_yearly_flat", Some("year"), None),
            sub_item("si_monthly_seat", Some("month"), None),
        ];
        let picked = pick_seat_item(&None, &items).unwrap();
        assert_eq!(picked.id, "si_monthly_seat");
    }

    #[test]
    fn pick_seat_errors_on_empty_items() {
        // StripeSubscriptionItem is intentionally not Debug, so unwrap_err
        // can't be used here — match on the result directly.
        match pick_seat_item(&None, &[]) {
            Err(BillingError::MalformedStripeResponse) => {}
            _ => panic!("expected MalformedStripeResponse"),
        }
    }

    // ---- invoice_subscription_id ----

    #[test]
    fn invoice_subscription_id_prefers_new_schema() {
        let invoice = json!({
            "parent": {"subscription_details": {"subscription": "sub_new"}},
            "subscription": "sub_legacy",
        });
        assert_eq!(
            invoice_subscription_id(&invoice).as_deref(),
            Some("sub_new")
        );
    }

    #[test]
    fn invoice_subscription_id_falls_back_to_legacy_top_level() {
        // 2024+ field absent — old field still used by older API versions.
        let invoice = json!({"subscription": "sub_legacy"});
        assert_eq!(
            invoice_subscription_id(&invoice).as_deref(),
            Some("sub_legacy")
        );
    }

    #[test]
    fn invoice_subscription_id_returns_none_when_neither_field_present() {
        let invoice = json!({"id": "in_test"});
        assert!(invoice_subscription_id(&invoice).is_none());
    }

    // ---- time_from_unix ----

    #[test]
    fn time_from_unix_round_trips_a_known_epoch() {
        let dt = time_from_unix(1_700_000_000).unwrap();
        assert_eq!(dt.timestamp(), 1_700_000_000);
    }
}
