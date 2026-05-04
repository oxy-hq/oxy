//! Public data-transfer types for `BillingService`.
//!
//! Split out so each operational module (`customers`, `webhook`, `provision`,
//! …) can import only the shapes it actually returns or accepts.

use entity::org_billing::BillingCycle;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct InvoiceDto {
    pub id: String,
    pub amount_due: i64,
    pub amount_paid: i64,
    pub currency: String,
    pub status: String,
    pub hosted_invoice_url: Option<String>,
    pub period_start: Option<i64>,
    pub period_end: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminPriceDto {
    pub id: String,
    pub nickname: Option<String>,
    pub unit_amount: i64,
    pub currency: String,
    pub interval: String,
    pub product_name: Option<String>,
    pub label: String,
    /// Pre-formatted price string for display. Handles tiered prices where
    /// `unit_amount` alone is `null` and the real amounts live in `tiers[]`.
    pub amount_display: String,
    /// Stripe billing scheme: `per_unit` (flat) or `tiered`.
    pub billing_scheme: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SubscriptionOverview {
    pub cycle: Option<BillingCycle>,
    pub items: Vec<AdminSubscriptionItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminOrgRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub owner_email: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub stripe_subscription_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminSubscriptionDetail {
    pub id: String,
    pub status: String,
    pub livemode: bool,
    pub created: Option<i64>,
    pub current_period_start: Option<i64>,
    pub current_period_end: Option<i64>,
    pub cancel_at_period_end: bool,
    pub collection_method: Option<String>,
    pub customer_id: Option<String>,
    pub items: Vec<AdminSubscriptionItem>,
    pub latest_invoice: Option<LatestInvoiceSummary>,
}

/// Snapshot of a subscription's most recent invoice. Subscriptions with
/// `collection_method=send_invoice` create a draft invoice that auto-finalizes
/// after ~1 hour — surfacing `status` + `auto_advance` lets the UI explain the
/// delay instead of looking like the email failed.
#[derive(Debug, Clone, Serialize)]
pub struct LatestInvoiceSummary {
    pub id: String,
    pub status: String,
    pub collection_method: Option<String>,
    pub hosted_invoice_url: Option<String>,
    pub invoice_pdf: Option<String>,
    pub amount_due: i64,
    pub amount_paid: i64,
    pub currency: String,
    pub auto_advance: Option<bool>,
    pub created: Option<i64>,
    pub due_date: Option<i64>,
    pub next_payment_attempt: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminSubscriptionItem {
    pub id: String,
    pub quantity: u64,
    pub price_id: String,
    pub price_nickname: Option<String>,
    pub unit_amount: i64,
    pub currency: String,
    pub interval: Option<String>,
    pub product_name: Option<String>,
    pub current_period_start: Option<i64>,
    pub current_period_end: Option<i64>,
    /// Pre-formatted price string. Falls back to first-tier amounts when the
    /// underlying Stripe price uses tiered billing (`unit_amount` is null).
    pub amount_display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvisionItemRole {
    Seat,
    Flat,
}

impl ProvisionItemRole {
    pub(super) fn metadata_value(self) -> &'static str {
        match self {
            Self::Seat => "seat",
            Self::Flat => "flat",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvisionItem {
    pub price_id: String,
    pub role: ProvisionItemRole,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvisionRequest {
    pub items: Vec<ProvisionItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvisionResponse {
    pub provisioned: bool,
    pub subscription_id: String,
    pub latest_invoice: Option<LatestInvoiceSummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvisionCheckoutRequest {
    pub items: Vec<ProvisionItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutSessionInfo {
    pub session_id: String,
    pub url: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct CheckoutCreated {
    pub session: CheckoutSessionInfo,
    pub owner_email: String,
    pub org_name: String,
}

pub enum ProvisionCheckoutOutcome {
    Created(CheckoutCreated),
    AlreadyPending(CheckoutSessionInfo),
}

#[derive(Debug, Clone)]
pub enum BillingNotification {
    PastDueEntered {
        org_id: Uuid,
        org_name: String,
        org_slug: String,
        owner_email: String,
        grace_ends_at: chrono::DateTime<chrono::Utc>,
    },
}

/// Result of looking up a Stripe webhook event by id before processing.
///
/// `AlreadySuccess` short-circuits the handler back to 200 — the event has
/// already been applied. Anything else means the caller should run apply.
/// All apply paths are idempotent.
pub enum LookupOutcome {
    AlreadySuccess,
    NeedsApply,
}
