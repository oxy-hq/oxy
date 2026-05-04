//! Minimal Stripe API response shapes.
//!
//! Each struct deserializes only the fields the service actually reads —
//! Stripe payloads are huge and we never want to fail on unknown ones, so
//! these are intentionally narrow and `pub(super)` only.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value as JsonValue;

#[derive(Deserialize)]
pub(super) struct StripeCustomer {
    pub(super) id: String,
}

#[derive(Deserialize)]
pub(super) struct StripePortalSession {
    pub(super) url: String,
}

#[derive(Deserialize)]
pub(super) struct StripeSubscription {
    pub(super) id: String,
    pub(super) status: String,
    // Optional: with `billing_mode=flexible` (mixed-interval support), Stripe
    // tracks the period on each item instead of at the subscription level.
    // We prefer the seat item's period; fall back to top-level for legacy.
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) current_period_start: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) current_period_end: Option<i64>,
    pub(super) items: StripeList<StripeSubscriptionItem>,
}

#[derive(Deserialize)]
pub(super) struct StripeSubscriptionItem {
    pub(super) id: String,
    pub(super) price: StripeSubItemPrice,
    pub(super) quantity: Option<u64>,
    #[serde(default)]
    pub(super) current_period_start: Option<i64>,
    #[serde(default)]
    pub(super) current_period_end: Option<i64>,
    #[serde(default)]
    pub(super) metadata: BTreeMap<String, String>,
}

#[derive(Deserialize)]
pub(super) struct StripeSubItemPrice {
    #[allow(dead_code)]
    pub(super) id: String,
    pub(super) recurring: Option<StripeRecurring>,
}

#[derive(Deserialize)]
pub(super) struct StripeRecurring {
    pub(super) interval: String,
}

#[derive(Deserialize)]
pub(super) struct StripeList<T> {
    pub(super) data: Vec<T>,
}

#[derive(Deserialize)]
pub(super) struct StripeCheckoutSession {
    pub(super) id: String,
    pub(super) url: Option<String>,
    pub(super) expires_at: i64,
    #[serde(default)]
    pub(super) status: Option<String>,
    #[serde(default)]
    pub(super) payment_status: Option<String>,
    #[serde(default)]
    pub(super) metadata: JsonValue,
}

#[derive(Deserialize)]
pub(super) struct StripeInvoice {
    pub(super) id: String,
    pub(super) amount_due: Option<i64>,
    pub(super) amount_paid: Option<i64>,
    pub(super) currency: Option<String>,
    pub(super) status: Option<String>,
    pub(super) hosted_invoice_url: Option<String>,
    pub(super) period_start: Option<i64>,
    pub(super) period_end: Option<i64>,
}
