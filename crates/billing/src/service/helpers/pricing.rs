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

/// Reject volume-tiered prices. We support `per_unit` (flat) and
/// `tiered` + `tiers_mode=graduated`; volume tiering charges every unit
/// at a single tier's rate, which doesn't compose with seat-included
/// platform-fee pricing the way graduated tiers do. Stripe omits
/// `billing_scheme` on some legacy responses, so treat missing as
/// `per_unit`.
pub(in crate::service) fn require_per_unit_or_graduated(
    price: &JsonValue,
    id: &str,
) -> Result<(), BillingError> {
    let scheme = price["billing_scheme"].as_str().unwrap_or("per_unit");
    if scheme == "per_unit" {
        return Ok(());
    }
    if scheme == "tiered" && price["tiers_mode"].as_str() == Some("graduated") {
        return Ok(());
    }
    Err(BillingError::UnsupportedPricingMode(id.to_string()))
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

/// Pure form of `BillingService::validate_provision_prices` — runs the
/// recurring / pricing-mode / interval checks against already-fetched
/// price JSON. Split from the async wrapper so it can be unit-tested
/// with synthesized JSON instead of mocking the Stripe HTTP client.
pub(in crate::service) fn validate_prices(
    seat_price: &JsonValue,
    seat_id: &str,
    flat_prices: &[(&JsonValue, &str)],
    enforce_matching_intervals: bool,
) -> Result<(), BillingError> {
    require_active_recurring(seat_price, seat_id)?;
    require_per_unit_or_graduated(seat_price, seat_id)?;
    for (flat_price, flat_id) in flat_prices {
        require_active_recurring(flat_price, flat_id)?;
        require_per_unit_or_graduated(flat_price, flat_id)?;
        if enforce_matching_intervals {
            require_matching_intervals(seat_price, seat_id, flat_price, flat_id)?;
        }
    }
    Ok(())
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
    params.push((
        format!("{prefix}[0][metadata][oxy_role]"),
        ProvisionItemRole::Seat.metadata_value().into(),
    ));
    for (idx, flat) in flat_items.iter().enumerate() {
        let i = idx + 1;
        params.push((format!("{prefix}[{i}][price]"), flat.price_id.clone()));
        // Flat add-ons are intentionally fixed at quantity=1 — they're
        // platform-level line items, not seat-scaled. A graduated-tiered
        // price assigned here will therefore always land in the first tier.
        // The seat-scaled "platform fee with N included seats" pattern
        // belongs on the seat item, not as a flat add-on.
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

    // ---- require_per_unit_or_graduated ----

    #[test]
    fn require_pricing_accepts_per_unit() {
        let p = json!({"billing_scheme": "per_unit"});
        assert!(require_per_unit_or_graduated(&p, "price_X").is_ok());
    }

    #[test]
    fn require_pricing_treats_missing_scheme_as_per_unit() {
        // Some legacy Stripe responses omit billing_scheme — must default to per_unit.
        let p = json!({});
        assert!(require_per_unit_or_graduated(&p, "price_X").is_ok());
    }

    #[test]
    fn require_pricing_accepts_graduated_tiered() {
        let p = json!({"billing_scheme": "tiered", "tiers_mode": "graduated"});
        assert!(require_per_unit_or_graduated(&p, "price_X").is_ok());
    }

    #[test]
    fn require_pricing_rejects_volume_tiered() {
        let p = json!({"billing_scheme": "tiered", "tiers_mode": "volume"});
        let err = require_per_unit_or_graduated(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::UnsupportedPricingMode(p) if p == "price_X"));
    }

    #[test]
    fn require_pricing_rejects_tiered_without_mode() {
        // Defensive: tiered with no tiers_mode is malformed; reject rather than accept.
        let p = json!({"billing_scheme": "tiered"});
        let err = require_per_unit_or_graduated(&p, "price_X").unwrap_err();
        assert!(matches!(err, BillingError::UnsupportedPricingMode(_)));
    }

    // ---- validate_prices ----

    fn per_unit(interval: &str) -> JsonValue {
        json!({
            "active": true,
            "billing_scheme": "per_unit",
            "recurring": {"interval": interval, "interval_count": 1},
        })
    }

    fn graduated(interval: &str) -> JsonValue {
        json!({
            "active": true,
            "billing_scheme": "tiered",
            "tiers_mode": "graduated",
            "recurring": {"interval": interval, "interval_count": 1},
        })
    }

    fn volume(interval: &str) -> JsonValue {
        json!({
            "active": true,
            "billing_scheme": "tiered",
            "tiers_mode": "volume",
            "recurring": {"interval": interval, "interval_count": 1},
        })
    }

    #[test]
    fn validate_prices_accepts_per_unit_seat_with_graduated_flat() {
        let seat = per_unit("month");
        let flat = graduated("month");
        let flat_refs = [(&flat, "price_flat")];
        assert!(validate_prices(&seat, "price_seat", &flat_refs, true).is_ok());
    }

    #[test]
    fn validate_prices_accepts_graduated_seat_with_per_unit_flat() {
        let seat = graduated("year");
        let flat = per_unit("year");
        let flat_refs = [(&flat, "price_flat")];
        assert!(validate_prices(&seat, "price_seat", &flat_refs, true).is_ok());
    }

    #[test]
    fn validate_prices_accepts_seat_only() {
        let seat = per_unit("month");
        assert!(validate_prices(&seat, "price_seat", &[], true).is_ok());
    }

    #[test]
    fn validate_prices_rejects_volume_seat() {
        let seat = volume("month");
        let err = validate_prices(&seat, "price_seat", &[], true).unwrap_err();
        assert!(matches!(err, BillingError::UnsupportedPricingMode(p) if p == "price_seat"));
    }

    #[test]
    fn validate_prices_rejects_volume_flat_among_mixed_items() {
        // Seat OK, first flat OK, second flat is volume → rejected with that flat's id.
        let seat = per_unit("month");
        let ok_flat = graduated("month");
        let bad_flat = volume("month");
        let flat_refs = [(&ok_flat, "price_ok"), (&bad_flat, "price_bad")];
        let err = validate_prices(&seat, "price_seat", &flat_refs, true).unwrap_err();
        assert!(matches!(err, BillingError::UnsupportedPricingMode(p) if p == "price_bad"));
    }

    #[test]
    fn validate_prices_rejects_inactive_seat() {
        let mut seat = per_unit("month");
        seat["active"] = json!(false);
        let err = validate_prices(&seat, "price_seat", &[], true).unwrap_err();
        assert!(matches!(err, BillingError::PriceInactive(p) if p == "price_seat"));
    }

    #[test]
    fn validate_prices_enforces_matching_intervals_when_required() {
        // Checkout flow: monthly seat + yearly flat must be rejected.
        let seat = per_unit("month");
        let flat = per_unit("year");
        let flat_refs = [(&flat, "price_flat")];
        let err = validate_prices(&seat, "price_seat", &flat_refs, true).unwrap_err();
        assert!(matches!(
            err,
            BillingError::MismatchedBillingInterval { .. }
        ));
    }

    #[test]
    fn validate_prices_allows_mismatched_intervals_when_not_enforced() {
        // Direct-API flow uses billing_mode=flexible — mismatched intervals OK.
        let seat = per_unit("month");
        let flat = per_unit("year");
        let flat_refs = [(&flat, "price_flat")];
        assert!(validate_prices(&seat, "price_seat", &flat_refs, false).is_ok());
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
