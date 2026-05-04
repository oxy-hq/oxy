//! Stripe payload → public DTO mappers.

use entity::org_billing::BillingStatus;
use serde_json::Value as JsonValue;

use crate::service::dto::{AdminSubscriptionItem, LatestInvoiceSummary};
use crate::service::helpers::format::format_amount_display;

/// Return `None` when `value` is missing/null OR is just an unexpanded ID
/// string (Stripe returns `latest_invoice` as either a string or an object
/// depending on whether `expand[]=latest_invoice` was sent).
pub(in crate::service) fn map_latest_invoice(value: &JsonValue) -> Option<LatestInvoiceSummary> {
    let obj = value.as_object()?;
    let id = obj.get("id")?.as_str()?.to_string();
    Some(LatestInvoiceSummary {
        id,
        status: obj
            .get("status")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_string(),
        collection_method: obj
            .get("collection_method")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        hosted_invoice_url: obj
            .get("hosted_invoice_url")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        invoice_pdf: obj
            .get("invoice_pdf")
            .and_then(JsonValue::as_str)
            .map(str::to_string),
        amount_due: obj
            .get("amount_due")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0),
        amount_paid: obj
            .get("amount_paid")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0),
        currency: obj
            .get("currency")
            .and_then(JsonValue::as_str)
            .unwrap_or("usd")
            .to_string(),
        auto_advance: obj.get("auto_advance").and_then(JsonValue::as_bool),
        created: obj.get("created").and_then(JsonValue::as_i64),
        due_date: obj.get("due_date").and_then(JsonValue::as_i64),
        next_payment_attempt: obj.get("next_payment_attempt").and_then(JsonValue::as_i64),
    })
}

pub(in crate::service) fn map_subscription_item(item: &JsonValue) -> AdminSubscriptionItem {
    let price = &item["price"];
    let unit_amount = price["unit_amount"].as_i64().unwrap_or(0);
    let currency = price["currency"].as_str().unwrap_or("usd").to_string();
    let amount_display = format_amount_display(price, unit_amount, &currency);
    AdminSubscriptionItem {
        id: item["id"].as_str().unwrap_or_default().to_string(),
        quantity: item["quantity"].as_u64().unwrap_or(0),
        price_id: price["id"].as_str().unwrap_or_default().to_string(),
        price_nickname: price["nickname"].as_str().map(str::to_string),
        unit_amount,
        currency,
        interval: price["recurring"]["interval"].as_str().map(str::to_string),
        product_name: price["product"]["name"].as_str().map(str::to_string),
        current_period_start: item["current_period_start"].as_i64(),
        current_period_end: item["current_period_end"].as_i64(),
        amount_display,
    }
}

/// Stripe subscription `status` strings → our `BillingStatus`.
pub(in crate::service) fn map_stripe_status_str(s: &str) -> BillingStatus {
    match s {
        "active" => BillingStatus::Active,
        // Trials aren't used in v1; treat as Active so a misconfigured
        // Stripe object can't strand an org in an unmapped state.
        "trialing" => BillingStatus::Active,
        "past_due" | "paused" => BillingStatus::PastDue,
        "unpaid" => BillingStatus::Unpaid,
        "canceled" | "incomplete_expired" => BillingStatus::Canceled,
        "incomplete" => BillingStatus::Incomplete,
        _ => BillingStatus::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- map_stripe_status_str ----

    #[test]
    fn map_status_known_values() {
        assert_eq!(map_stripe_status_str("active"), BillingStatus::Active);
        assert_eq!(map_stripe_status_str("past_due"), BillingStatus::PastDue);
        assert_eq!(map_stripe_status_str("paused"), BillingStatus::PastDue);
        assert_eq!(map_stripe_status_str("unpaid"), BillingStatus::Unpaid);
        assert_eq!(map_stripe_status_str("canceled"), BillingStatus::Canceled);
        assert_eq!(
            map_stripe_status_str("incomplete_expired"),
            BillingStatus::Canceled
        );
        assert_eq!(
            map_stripe_status_str("incomplete"),
            BillingStatus::Incomplete
        );
    }

    #[test]
    fn map_status_trialing_is_treated_as_active() {
        // Trials aren't a v1 product surface, but a misconfigured Stripe
        // object shouldn't strand an org in an unmapped state.
        assert_eq!(map_stripe_status_str("trialing"), BillingStatus::Active);
    }

    #[test]
    fn map_status_unknown_falls_back_to_active() {
        // Any future Stripe status we haven't seen yet should not lock
        // users out — failing closed here is worse than failing open.
        assert_eq!(
            map_stripe_status_str("some_future_status"),
            BillingStatus::Active
        );
        assert_eq!(map_stripe_status_str(""), BillingStatus::Active);
    }

    // ---- map_latest_invoice ----

    #[test]
    fn map_latest_invoice_returns_none_for_unexpanded_string() {
        // Stripe returns latest_invoice as a bare ID string when not expanded.
        let bare = json!("in_unexpanded");
        assert!(map_latest_invoice(&bare).is_none());
    }

    #[test]
    fn map_latest_invoice_returns_none_for_null() {
        assert!(map_latest_invoice(&json!(null)).is_none());
    }

    #[test]
    fn map_latest_invoice_extracts_expanded_object_fields() {
        let inv = json!({
            "id": "in_123",
            "status": "open",
            "collection_method": "send_invoice",
            "hosted_invoice_url": "https://stripe.example/i/123",
            "invoice_pdf": "https://stripe.example/i/123.pdf",
            "amount_due": 10000,
            "amount_paid": 0,
            "currency": "usd",
            "auto_advance": true,
            "created": 1_700_000_000,
            "due_date": 1_700_500_000,
            "next_payment_attempt": 1_700_100_000,
        });
        let mapped = map_latest_invoice(&inv).unwrap();
        assert_eq!(mapped.id, "in_123");
        assert_eq!(mapped.status, "open");
        assert_eq!(mapped.collection_method.as_deref(), Some("send_invoice"));
        assert_eq!(mapped.amount_due, 10000);
        assert_eq!(mapped.amount_paid, 0);
        assert_eq!(mapped.currency, "usd");
        assert_eq!(mapped.auto_advance, Some(true));
        assert_eq!(mapped.created, Some(1_700_000_000));
    }
}
