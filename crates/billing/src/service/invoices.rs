//! Invoice listing + customer-facing subscription overview.
//!
//! Houses the shared `fetch_subscription_with_items` helper used by both
//! the customer overview here and the admin detail view in `admin.rs`.

use entity::org_billing::BillingCycle;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::dto::{AdminSubscriptionItem, InvoiceDto, SubscriptionOverview};
use crate::service::helpers::mappers::map_subscription_item;
use crate::service::stripe_shapes::{StripeInvoice, StripeList};

impl BillingService {
    pub async fn list_invoices(&self, org_id: Uuid) -> Result<Vec<InvoiceDto>, BillingError> {
        let row = self.load_billing(org_id).await?;
        let Some(cid) = row.stripe_customer_id else {
            return Ok(vec![]);
        };
        let url = format!("/v1/invoices?customer={cid}&limit=20");
        let resp: StripeList<StripeInvoice> = self.client.get(&url).await?;
        Ok(resp
            .data
            .into_iter()
            .map(|inv| InvoiceDto {
                id: inv.id,
                amount_due: inv.amount_due.unwrap_or(0),
                amount_paid: inv.amount_paid.unwrap_or(0),
                currency: inv.currency.unwrap_or_else(|| "usd".into()),
                status: inv.status.unwrap_or_default(),
                hosted_invoice_url: inv.hosted_invoice_url,
                period_start: inv.period_start,
                period_end: inv.period_end,
            })
            .collect())
    }

    /// Subscription overview embedded in the billing summary: seat-item
    /// billing cycle plus the live item list (product, quantity, price,
    /// per-item period). Fetched from Stripe so the panel always reflects
    /// the current subscription state.
    pub async fn current_subscription_overview(
        &self,
        org_id: Uuid,
    ) -> Result<SubscriptionOverview, BillingError> {
        let row = self.load_billing(org_id).await?;
        let Some(sub_id) = row.stripe_subscription_id else {
            return Ok(SubscriptionOverview::default());
        };
        let sub = self.fetch_subscription_with_items(&sub_id, false).await?;

        let items: Vec<AdminSubscriptionItem> = sub["items"]["data"]
            .as_array()
            .map(|arr| arr.iter().map(map_subscription_item).collect())
            .unwrap_or_default();

        // Use the shortest interval across items (Monthly < Annual). Matched
        // intervals are enforced at provision time so every item normally
        // resolves to the same cycle; this matters only for legacy mixed-mode.
        let cycle = items
            .iter()
            .filter_map(|i| i.interval.as_deref())
            .filter_map(BillingCycle::from_stripe_interval)
            .min_by_key(|c| match c {
                BillingCycle::Monthly => 0,
                BillingCycle::Annual => 1,
            });

        Ok(SubscriptionOverview { cycle, items })
    }

    /// Fetch a Stripe subscription with item/price/product expansions, used
    /// by both the customer overview and the admin detail dialog. When
    /// `expand_invoice` is true, also expand `latest_invoice`.
    pub(super) async fn fetch_subscription_with_items(
        &self,
        sub_id: &str,
        expand_invoice: bool,
    ) -> Result<JsonValue, BillingError> {
        let mut path = format!(
            "/v1/subscriptions/{sub_id}\
             ?expand[]=items.data.price.product\
             &expand[]=items.data.price.tiers"
        );
        if expand_invoice {
            path.push_str("&expand[]=latest_invoice");
        }
        self.client.get(&path).await
    }
}
