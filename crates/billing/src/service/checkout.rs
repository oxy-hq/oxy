//! Checkout-Sessions provisioning flow + pending-session management.
//!
//! Stripe is the source of truth for pending session state — no DB columns
//! are added. Pending lookups query `/v1/checkout/sessions?status=open`.

use std::collections::BTreeMap;

use entity::org_billing::BillingStatus;
use reqwest::Method;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::dto::{
    CheckoutCreated, CheckoutSessionInfo, ProvisionCheckoutOutcome, ProvisionCheckoutRequest,
    ProvisionItem,
};
use crate::service::helpers::pricing::{push_subscription_items, split_provision_items};
use crate::service::stripe_shapes::{StripeCheckoutSession, StripeList};

impl BillingService {
    /// Provision via a Stripe Checkout Session. The customer completes
    /// billing-address + tax-id + payment in a single hosted flow; the
    /// subscription is created automatically when they confirm.
    ///
    /// Stripe is the source of truth for pending session state — no DB
    /// columns are added. If a session is already open for this customer,
    /// returns [`ProvisionCheckoutOutcome::AlreadyPending`] so the caller
    /// can offer resend / cancel-and-recreate.
    ///
    /// Promotion of `org.status` to `Active` happens asynchronously via the
    /// existing `customer.subscription.created` webhook handler.
    pub async fn provision_via_checkout(
        &self,
        org_id: Uuid,
        req: ProvisionCheckoutRequest,
    ) -> Result<ProvisionCheckoutOutcome, BillingError> {
        let billing = self.load_billing(org_id).await?;
        if billing.status != BillingStatus::Incomplete {
            return Err(BillingError::AlreadyProvisioned);
        }

        let (owner_email, org_name, slug) = self.org_owner_and_name(org_id).await?;

        let (seat_item, flat_items) = split_provision_items(&req.items)?;
        // Checkout Sessions reject mixed billing intervals even with
        // `billing_mode=flexible`. Enforce matching here.
        self.validate_provision_prices(seat_item, &flat_items, true)
            .await?;

        let quantity = self.member_count(org_id).await?.max(1) as u64;

        let customer_id = self
            .ensure_customer(org_id, &owner_email, &org_name)
            .await?;

        if let Some(existing) = self.find_open_checkout_session(&customer_id).await? {
            return Ok(ProvisionCheckoutOutcome::AlreadyPending(existing));
        }

        let session = self
            .create_checkout_session(
                &customer_id,
                org_id,
                &slug,
                quantity,
                seat_item,
                &flat_items,
            )
            .await?;

        Ok(ProvisionCheckoutOutcome::Created(CheckoutCreated {
            session,
            owner_email,
            org_name,
        }))
    }

    /// Look up the currently-open Checkout Session for an org, if any.
    /// Used by the admin "view pending" / resend flows. Returns `None`
    /// when there is no Stripe customer yet or no open session.
    pub async fn get_pending_checkout(
        &self,
        org_id: Uuid,
    ) -> Result<Option<CheckoutSessionInfo>, BillingError> {
        let billing = self.load_billing(org_id).await?;
        let Some(customer_id) = billing.stripe_customer_id else {
            return Ok(None);
        };
        self.find_open_checkout_session(&customer_id).await
    }

    /// Expire the org's currently-open Checkout Session, if any. After
    /// cancellation the admin can call `provision_via_checkout` again to
    /// create a fresh session. No-op when there is no open session.
    pub async fn cancel_pending_checkout(&self, org_id: Uuid) -> Result<(), BillingError> {
        let billing = self.load_billing(org_id).await?;
        let Some(customer_id) = billing.stripe_customer_id else {
            return Err(BillingError::NoPendingCheckout);
        };
        let Some(info) = self.find_open_checkout_session(&customer_id).await? else {
            return Err(BillingError::NoPendingCheckout);
        };
        let path = format!("/v1/checkout/sessions/{}/expire", info.session_id);
        let _: JsonValue = self
            .client
            .form(Method::POST, &path, &BTreeMap::new(), None)
            .await?;
        Ok(())
    }

    /// Look up a Checkout Session by id and return whether it has been paid.
    ///
    /// Used by the customer-facing success page to confirm the redirect
    /// is real before showing the "activating subscription" spinner.
    /// Always validates that the session's `metadata.oxy_org_id` matches
    /// the calling org so a session id cannot be replayed cross-org.
    pub async fn verify_checkout_session(
        &self,
        org_id: Uuid,
        session_id: &str,
    ) -> Result<bool, BillingError> {
        if !session_id.starts_with("cs_") {
            return Err(BillingError::Forbidden("invalid session id".into()));
        }
        let path = format!("/v1/checkout/sessions/{session_id}");
        let session: StripeCheckoutSession = self.client.get(&path).await?;
        let session_org = session
            .metadata
            .get("oxy_org_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if session_org != org_id.to_string() {
            return Err(BillingError::Forbidden(
                "session does not belong to this org".into(),
            ));
        }
        // For subscription mode, `payment_status` is "paid" once the first
        // invoice is paid, or "no_payment_required" for trials / manual
        // collection. Either is a valid "checkout finished" signal.
        let paid = matches!(session.status.as_deref(), Some("complete"))
            && matches!(
                session.payment_status.as_deref(),
                Some("paid") | Some("no_payment_required")
            );
        Ok(paid)
    }

    async fn find_open_checkout_session(
        &self,
        customer_id: &str,
    ) -> Result<Option<CheckoutSessionInfo>, BillingError> {
        let path = format!("/v1/checkout/sessions?customer={customer_id}&status=open&limit=1");
        let list: StripeList<StripeCheckoutSession> = self.client.get(&path).await?;
        let Some(s) = list.data.into_iter().next() else {
            return Ok(None);
        };
        let url = s.url.ok_or(BillingError::MalformedStripeResponse)?;
        Ok(Some(CheckoutSessionInfo {
            session_id: s.id,
            url,
            expires_at: s.expires_at,
        }))
    }

    async fn create_checkout_session(
        &self,
        customer_id: &str,
        org_id: Uuid,
        org_slug: &str,
        quantity: u64,
        seat_item: &ProvisionItem,
        flat_items: &[&ProvisionItem],
    ) -> Result<CheckoutSessionInfo, BillingError> {
        // `{CHECKOUT_SESSION_ID}` is a Stripe template variable substituted
        // at redirect time — must be left as a literal here.
        let success_url = format!(
            "{}/{}/billing/checkout-success?session_id={{CHECKOUT_SESSION_ID}}",
            self.client.public_url(),
            org_slug,
        );
        let cancel_url = format!(
            "{}/{}/billing/checkout-cancelled",
            self.client.public_url(),
            org_slug,
        );
        let mut params: Vec<(String, String)> = vec![
            ("mode".into(), "subscription".into()),
            ("customer".into(), customer_id.to_string()),
            ("success_url".into(), success_url),
            ("cancel_url".into(), cancel_url),
            ("billing_address_collection".into(), "required".into()),
            ("tax_id_collection[enabled]".into(), "true".into()),
            ("customer_update[address]".into(), "auto".into()),
            ("customer_update[name]".into(), "auto".into()),
            (
                "subscription_data[billing_mode][type]".into(),
                "flexible".into(),
            ),
            (
                "subscription_data[metadata][oxy_org_id]".into(),
                org_id.to_string(),
            ),
            // Session-level metadata so the success page can verify the
            // session belongs to the org in the URL (spoof guard).
            ("metadata[oxy_org_id]".into(), org_id.to_string()),
            ("automatic_tax[enabled]".into(), "true".into()),
        ];
        // Stripe propagates line_items metadata to the underlying
        // subscription items in `mode=subscription`, giving the webhook
        // a deterministic way to identify the seat item — see
        // `pick_seat_item`.
        push_subscription_items(
            &mut params,
            "line_items",
            &seat_item.price_id,
            quantity,
            flat_items,
        );

        let idem_key = Uuid::new_v4().to_string();
        let session: StripeCheckoutSession = self
            .client
            .form_repeated(
                Method::POST,
                "/v1/checkout/sessions",
                &params,
                Some(&idem_key),
            )
            .await?;
        let url = session.url.ok_or(BillingError::MalformedStripeResponse)?;
        Ok(CheckoutSessionInfo {
            session_id: session.id,
            url,
            expires_at: session.expires_at,
        })
    }
}
