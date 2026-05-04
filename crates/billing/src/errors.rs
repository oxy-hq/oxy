use thiserror::Error;

#[derive(Debug, Error)]
pub enum BillingError {
    #[error("stripe api error ({status}): {body}")]
    Stripe { status: u16, body: String },
    #[error("stripe transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("database error: {0}")]
    Db(#[from] sea_orm::DbErr),
    #[error("webhook signature invalid")]
    InvalidSignature,
    #[error("webhook event stale (> 5 min old)")]
    StaleWebhook,
    #[error("webhook event malformed: {0}")]
    MalformedEvent(String),
    #[error("malformed stripe response")]
    MalformedStripeResponse,
    #[error("unknown price id: {0}")]
    UnknownPrice(String),
    #[error("price {0} is inactive")]
    PriceInactive(String),
    #[error("price {0} is not recurring")]
    PriceNotRecurring(String),
    #[error("price {0} is not a flat (per_unit) price")]
    PriceNotFlat(String),
    #[error(
        "billing intervals do not match: seat price {seat_price_id} is {seat_interval}, price {other_price_id} is {other_interval}"
    )]
    MismatchedBillingInterval {
        seat_price_id: String,
        seat_interval: String,
        other_price_id: String,
        other_interval: String,
    },
    #[error("at least one price item is required")]
    NoProvisionItems,
    #[error("days_until_due must be between {min} and {max} (got {got})")]
    InvalidDaysUntilDue { got: u32, min: u32, max: u32 },
    #[error("exactly one item must be marked as seat-sync (got {0})")]
    InvalidSeatItemCount(usize),
    #[error("price {0} appears more than once")]
    DuplicatePriceItem(String),
    #[error("subscription is already provisioned for this org")]
    AlreadyProvisioned,
    #[error("no pending checkout session for this org")]
    NoPendingCheckout,
    #[error("stripe customer is missing for this org")]
    MissingCustomer,
    #[error("invalid status filter: {0}")]
    InvalidStatus(String),
    #[error("org_billing row missing for {0} (data drift)")]
    OrgBillingMissing(uuid::Uuid),
    #[error("org owner not found")]
    OrgOwnerNotFound,
    #[error("no subscription found for org")]
    NoSubscription,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("config error: {0}")]
    Config(String),
}
