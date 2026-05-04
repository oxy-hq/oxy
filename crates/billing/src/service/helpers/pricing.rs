//! Price-validation rules + `ProvisionItem` partitioning.
//!
//! All admin-supplied price IDs flow through these checks before reaching
//! Stripe — they're the trust boundary between the admin form and any
//! `/v1/subscriptions` or `/v1/checkout/sessions` POST.

use serde_json::Value as JsonValue;

use crate::errors::BillingError;
use crate::service::dto::{ProvisionItem, ProvisionItemRole};

pub(in crate::service) fn require_active_recurring(
    price: &JsonValue,
    id: &str,
) -> Result<(), BillingError> {
    if price["active"].as_bool() != Some(true) {
        return Err(BillingError::PriceInactive(id.to_string()));
    }
    if price["recurring"].is_null() {
        return Err(BillingError::PriceNotRecurring(id.to_string()));
    }
    Ok(())
}

/// Reject tiered prices. Seat sync and flat add-ons assume a fixed per-unit
/// amount; tiered pricing would silently change the invoice math at the
/// wrong layer. Stripe omits `billing_scheme` on some legacy responses, so
/// treat missing as `per_unit`.
pub(in crate::service) fn require_flat_billing(
    price: &JsonValue,
    id: &str,
) -> Result<(), BillingError> {
    let scheme = price["billing_scheme"].as_str().unwrap_or("per_unit");
    if scheme != "per_unit" {
        return Err(BillingError::PriceNotFlat(id.to_string()));
    }
    Ok(())
}

pub(in crate::service) fn recurring_interval_label(price: &JsonValue) -> String {
    let interval = price["recurring"]["interval"].as_str().unwrap_or("?");
    let count = price["recurring"]["interval_count"].as_u64().unwrap_or(1);
    if count == 1 {
        interval.to_string()
    } else {
        format!("{count} {interval}")
    }
}

pub(in crate::service) fn require_matching_intervals(
    seat: &JsonValue,
    seat_id: &str,
    other: &JsonValue,
    other_id: &str,
) -> Result<(), BillingError> {
    let seat_unit = seat["recurring"]["interval"].as_str();
    let other_unit = other["recurring"]["interval"].as_str();
    let seat_count = seat["recurring"]["interval_count"].as_u64().unwrap_or(1);
    let other_count = other["recurring"]["interval_count"].as_u64().unwrap_or(1);
    if seat_unit == other_unit && seat_count == other_count {
        return Ok(());
    }
    Err(BillingError::MismatchedBillingInterval {
        seat_price_id: seat_id.to_string(),
        seat_interval: recurring_interval_label(seat),
        other_price_id: other_id.to_string(),
        other_interval: recurring_interval_label(other),
    })
}

/// Validate the items array supplied by the admin form: must contain at
/// least one item, exactly one with `role = seat`, and no duplicate price
/// ids. Returns the seat item and the remaining flat items in input order.
pub(in crate::service) fn split_provision_items(
    items: &[ProvisionItem],
) -> Result<(&ProvisionItem, Vec<&ProvisionItem>), BillingError> {
    if items.is_empty() {
        return Err(BillingError::NoProvisionItems);
    }
    let mut seen = std::collections::HashSet::new();
    for item in items {
        if !seen.insert(item.price_id.as_str()) {
            return Err(BillingError::DuplicatePriceItem(item.price_id.clone()));
        }
    }
    let seats: Vec<&ProvisionItem> = items
        .iter()
        .filter(|i| i.role == ProvisionItemRole::Seat)
        .collect();
    if seats.len() != 1 {
        return Err(BillingError::InvalidSeatItemCount(seats.len()));
    }
    let flats: Vec<&ProvisionItem> = items
        .iter()
        .filter(|i| i.role == ProvisionItemRole::Flat)
        .collect();
    Ok((seats[0], flats))
}

/// Append seat + flat items as Stripe form params. Used by both the direct
/// `/v1/subscriptions` create flow (`prefix = "items"`) and the Checkout
/// Sessions flow (`prefix = "line_items"`).
pub(in crate::service) fn push_subscription_items(
    params: &mut Vec<(String, String)>,
    prefix: &str,
    seat_price_id: &str,
    quantity: u64,
    flat_items: &[&ProvisionItem],
) {
    params.push((format!("{prefix}[0][price]"), seat_price_id.to_string()));
    params.push((format!("{prefix}[0][quantity]"), quantity.to_string()));
    // Tag items so the webhook can identify the seat item without
    // racing the local UPDATE that persists the seat item id. See
    // `pick_seat_item` for the lookup order.
    params.push((format!("{prefix}[0][metadata][oxy_role]"), "seat".into()));
    for (idx, flat) in flat_items.iter().enumerate() {
        let i = idx + 1;
        params.push((format!("{prefix}[{i}][price]"), flat.price_id.clone()));
        params.push((format!("{prefix}[{i}][quantity]"), "1".into()));
        params.push((
            format!("{prefix}[{i}][metadata][oxy_role]"),
            ProvisionItemRole::Flat.metadata_value().into(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(price_id: &str, role: ProvisionItemRole) -> ProvisionItem {
        ProvisionItem {
            price_id: price_id.into(),
            role,
        }
    }

    // ---- split_provision_items ----

    #[test]
    fn split_rejects_empty() {
        let err = split_provision_items(&[]).unwrap_err();
        assert!(matches!(err, BillingError::NoProvisionItems));
    }

    #[test]
    fn split_rejects_duplicate_price_ids() {
        let items = vec![
            item("price_X", ProvisionItemRole::Seat),
            item("price_X", ProvisionItemRole::Flat),
        ];
        let err = split_provision_items(&items).unwrap_err();
        assert!(matches!(err, BillingError::DuplicatePriceItem(p) if p == "price_X"));
    }

    #[test]
    fn split_rejects_zero_seats() {
        let items = vec![
            item("price_A", ProvisionItemRole::Flat),
            item("price_B", ProvisionItemRole::Flat),
        ];
        let err = split_provision_items(&items).unwrap_err();
        assert!(matches!(err, BillingError::InvalidSeatItemCount(0)));
    }

    #[test]
    fn split_rejects_multiple_seats() {
        let items = vec![
            item("price_A", ProvisionItemRole::Seat),
            item("price_B", ProvisionItemRole::Seat),
        ];
        let err = split_provision_items(&items).unwrap_err();
        assert!(matches!(err, BillingError::InvalidSeatItemCount(2)));
    }

    #[test]
    fn split_returns_seat_and_flats_in_input_order() {
        let items = vec![
            item("price_flat1", ProvisionItemRole::Flat),
            item("price_seat", ProvisionItemRole::Seat),
            item("price_flat2", ProvisionItemRole::Flat),
        ];
        let (seat, flats) = split_provision_items(&items).unwrap();
        assert_eq!(seat.price_id, "price_seat");
        assert_eq!(flats.len(), 2);
        assert_eq!(flats[0].price_id, "price_flat1");
        assert_eq!(flats[1].price_id, "price_flat2");
    }

    #[test]
    fn split_allows_only_seat_no_flats() {
        let items = vec![item("price_seat", ProvisionItemRole::Seat)];
        let (seat, flats) = split_provision_items(&items).unwrap();
        assert_eq!(seat.price_id, "price_seat");
        assert!(flats.is_empty());
    }

    // ---- require_active_recurring ----

    #[test]
    fn require_active_recurring_accepts_active_recurring() {
        let p = json!({"active": true, "recurring": {"interval": "month"}});
        assert!(require_active_recurring(&p, "price_X").is_ok());
    }

    #[test]
    fn require_active_recurring_rejects_inactive() {
        let p = json!({"active": false, "recurring": {"interval": "month"}});
        let err = require_active_recurring(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::PriceInactive(p) if p == "price_X"));
    }

    #[test]
    fn require_active_recurring_rejects_missing_active_field() {
        // Missing `active` reads as null, which != Some(true) — defensively rejected.
        let p = json!({"recurring": {"interval": "month"}});
        let err = require_active_recurring(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::PriceInactive(_)));
    }

    #[test]
    fn require_active_recurring_rejects_one_time_price() {
        let p = json!({"active": true, "recurring": null});
        let err = require_active_recurring(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::PriceNotRecurring(p) if p == "price_X"));
    }

    // ---- require_flat_billing ----

    #[test]
    fn require_flat_billing_accepts_per_unit() {
        let p = json!({"billing_scheme": "per_unit"});
        assert!(require_flat_billing(&p, "price_X").is_ok());
    }

    #[test]
    fn require_flat_billing_treats_missing_as_per_unit() {
        // Some legacy Stripe responses omit billing_scheme — must default to per_unit.
        let p = json!({});
        assert!(require_flat_billing(&p, "price_X").is_ok());
    }

    #[test]
    fn require_flat_billing_rejects_tiered() {
        let p = json!({"billing_scheme": "tiered"});
        let err = require_flat_billing(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::PriceNotFlat(p) if p == "price_X"));
    }

    // ---- require_matching_intervals ----

    #[test]
    fn intervals_match_when_unit_and_count_equal() {
        let seat = json!({"recurring": {"interval": "month", "interval_count": 1}});
        let other = json!({"recurring": {"interval": "month", "interval_count": 1}});
        assert!(require_matching_intervals(&seat, "seat_id", &other, "other_id").is_ok());
    }

    #[test]
    fn intervals_mismatch_when_units_differ() {
        let seat = json!({"recurring": {"interval": "month", "interval_count": 1}});
        let other = json!({"recurring": {"interval": "year", "interval_count": 1}});
        let err = require_matching_intervals(&seat, "seat_id", &other, "other_id").unwrap_err();
        match err {
            BillingError::MismatchedBillingInterval {
                seat_price_id,
                other_price_id,
                ..
            } => {
                assert_eq!(seat_price_id, "seat_id");
                assert_eq!(other_price_id, "other_id");
            }
            _ => panic!("expected MismatchedBillingInterval"),
        }
    }

    #[test]
    fn intervals_mismatch_when_counts_differ() {
        // Same unit, different multiplier: monthly vs every-3-months still mismatches.
        let seat = json!({"recurring": {"interval": "month", "interval_count": 1}});
        let other = json!({"recurring": {"interval": "month", "interval_count": 3}});
        let err = require_matching_intervals(&seat, "seat_id", &other, "other_id").unwrap_err();
        assert!(matches!(
            err,
            BillingError::MismatchedBillingInterval { .. }
        ));
    }
}
