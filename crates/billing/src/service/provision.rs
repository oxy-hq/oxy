//! Direct-API provisioning: `/v1/subscriptions` POST.
//!
//! Used by the admin "provision now" button when the customer has already
//! shared payment details out-of-band. For self-serve where the customer
//! pays inline, see [`crate::service::checkout`].

use chrono::Utc;
use entity::org_billing::{self, BillingStatus};
use reqwest::Method;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::dto::{ProvisionItem, ProvisionRequest, ProvisionResponse};
use crate::service::helpers::extract::{extract_item_id_for_price, extract_period};
use crate::service::helpers::mappers::map_latest_invoice;
use crate::service::helpers::pricing::{
    push_subscription_items, require_active_recurring, require_flat_billing,
    require_matching_intervals, split_provision_items,
};

impl BillingService {
    /// Sales-led: create Stripe Customer + Subscription for an `Incomplete`
    /// org. Caller (admin route) supplies Price IDs admin selected from the
    /// dropdown. `metadata.oxy_org_id` is set server-side so admin cannot
    /// forget it.
    pub async fn provision_subscription(
        &self,
        org_id: Uuid,
        req: ProvisionRequest,
    ) -> Result<ProvisionResponse, BillingError> {
        let billing = self.load_billing(org_id).await?;
        if billing.status != BillingStatus::Incomplete {
            return Err(BillingError::AlreadyProvisioned);
        }

        let (owner_email, org_name, _org_slug) = self.org_owner_and_name(org_id).await?;

        let (seat_item, flat_items) = split_provision_items(&req.items)?;
        // Direct API supports mixed intervals via `billing_mode=flexible`,
        // so no interval-matching check here (unlike checkout).
        self.validate_provision_prices(seat_item, &flat_items, false)
            .await?;

        let quantity = self.member_count(org_id).await?.max(1) as u64;

        let customer_id = self
            .ensure_customer(org_id, &owner_email, &org_name)
            .await?;

        // `automatic_tax` is intentionally NOT enabled here. It requires the
        // Stripe Customer to have a tax-eligible address, which a freshly
        // provisioned Customer lacks (we only pass email + name + metadata
        // at create time). Admin enables tax later via the Stripe Dashboard
        // once the customer fills in their address through the restricted
        // Customer Portal (`features.customer_update.allowed_updates`
        // includes `address` + `tax_id` — see service::bootstrap_billing_portal_config).
        let mut params: Vec<(String, String)> = vec![
            ("customer".into(), customer_id.clone()),
            ("collection_method".into(), "send_invoice".into()),
            ("days_until_due".into(), "30".into()),
            ("billing_mode[type]".into(), "flexible".into()),
            ("metadata[oxy_org_id]".into(), org_id.to_string()),
            // Save the card used to pay the first invoice as the customer's
            // default payment method. After `invoice.payment_succeeded` for
            // the first invoice (`billing_reason=subscription_create`), we
            // flip `collection_method` to `charge_automatically` so renewals
            // auto-charge instead of re-emailing invoices.
            (
                "payment_settings[save_default_payment_method]".into(),
                "on_subscription".into(),
            ),
            // Expand so the response carries the draft invoice snapshot we
            // return to the admin UI (status + hosted URL).
            ("expand[]".into(), "latest_invoice".into()),
        ];
        push_subscription_items(
            &mut params,
            "items",
            &seat_item.price_id,
            quantity,
            &flat_items,
        );

        // Per Stripe docs: idempotency keys should be V4 UUIDs (or random
        // strings with enough entropy), not deterministic-from-params, and
        // must not encode identifiers like org_id or price_id. Duplicate
        // provisioning is prevented separately by the `status != Incomplete`
        // guard at the top of this method — the second call sees the org
        // is already Active and returns `AlreadyProvisioned`.
        let idem_key = Uuid::new_v4().to_string();
        let subscription: JsonValue = self
            .client
            .form_repeated(Method::POST, "/v1/subscriptions", &params, Some(&idem_key))
            .await?;

        let subscription_id = subscription["id"]
            .as_str()
            .ok_or(BillingError::MalformedStripeResponse)?
            .to_string();
        let seat_item_id = extract_item_id_for_price(&subscription, &seat_item.price_id)?;
        let (period_start, period_end) = extract_period(&subscription);

        // Persist subscription IDs immediately so the webhook race converges
        // even if `subscription.created` lands before this returns.
        let row = self.load_billing(org_id).await?;
        let mut am: org_billing::ActiveModel = row.into();
        am.stripe_customer_id = Set(Some(customer_id));
        am.stripe_subscription_id = Set(Some(subscription_id.clone()));
        am.stripe_subscription_seat_item_id = Set(Some(seat_item_id));
        am.seats_paid = Set(quantity as i32);
        if let Some(t) = period_start {
            am.current_period_start = Set(Some(t));
        }
        if let Some(t) = period_end {
            am.current_period_end = Set(Some(t));
        }
        am.updated_at = Set(Utc::now().into());
        am.update(&self.db).await?;

        let latest_invoice = map_latest_invoice(&subscription["latest_invoice"]);

        Ok(ProvisionResponse {
            provisioned: true,
            subscription_id,
            latest_invoice,
        })
    }

    /// Validate seat + flat prices for both provision flows. Each price must
    /// be active recurring and `per_unit` (flat) — tiered pricing would
    /// silently change invoice math at the wrong layer. When
    /// `enforce_matching_intervals` is true (Checkout flow), every flat item
    /// must share the seat's interval — Stripe Checkout rejects mixed
    /// intervals even with `billing_mode=flexible`.
    pub(super) async fn validate_provision_prices(
        &self,
        seat_item: &ProvisionItem,
        flat_items: &[&ProvisionItem],
        enforce_matching_intervals: bool,
    ) -> Result<(), BillingError> {
        let seat_price = self.fetch_price(&seat_item.price_id).await?;
        require_active_recurring(&seat_price, &seat_item.price_id)?;
        require_flat_billing(&seat_price, &seat_item.price_id)?;
        for flat in flat_items {
            let flat_price = self.fetch_price(&flat.price_id).await?;
            require_active_recurring(&flat_price, &flat.price_id)?;
            require_flat_billing(&flat_price, &flat.price_id)?;
            if enforce_matching_intervals {
                require_matching_intervals(
                    &seat_price,
                    &seat_item.price_id,
                    &flat_price,
                    &flat.price_id,
                )?;
            }
        }
        Ok(())
    }

    pub(super) async fn fetch_price(&self, id: &str) -> Result<JsonValue, BillingError> {
        self.client.get(&format!("/v1/prices/{id}")).await
    }
}
