//! Stripe Customer Portal session creation + restricted-config bootstrap.

use std::collections::BTreeMap;

use reqwest::Method;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::stripe_shapes::StripePortalSession;

const RESTRICTED_PORTAL_FEATURE_NAME: &str = "oxy-billing-restricted-v1";

impl BillingService {
    pub async fn create_portal_session(&self, org_id: Uuid) -> Result<String, BillingError> {
        let row = self.load_billing(org_id).await?;
        let customer_id = row
            .stripe_customer_id
            .clone()
            .ok_or(BillingError::NoSubscription)?;

        let (_, _, slug) = self.org_owner_and_name(org_id).await?;
        let portal_config_id = self.bootstrap_billing_portal_config().await?;
        // Lands on the org dispatcher which routes the user into their
        // most-recent workspace.
        let return_url = format!("{}/{}", self.client.public_url(), slug);
        let mut params = BTreeMap::new();
        params.insert("customer".into(), customer_id);
        params.insert("return_url".into(), return_url);
        if !portal_config_id.is_empty() {
            params.insert("configuration".into(), portal_config_id);
        }
        let key = format!("portal:{org_id}");
        let session: StripePortalSession = self
            .client
            .form(
                Method::POST,
                "/v1/billing_portal/sessions",
                &params,
                Some(&key),
            )
            .await?;
        Ok(session.url)
    }

    /// Idempotent: looks up an existing config tagged with our feature name;
    /// if none, creates one. Returns the configuration id (empty string on
    /// best-effort failure paths so callers default to Stripe's default).
    pub async fn bootstrap_billing_portal_config(&self) -> Result<String, BillingError> {
        let existing: JsonValue = self
            .client
            .get("/v1/billing_portal/configurations?is_default=false&active=true&limit=100")
            .await
            .unwrap_or_else(|_| serde_json::json!({ "data": [] }));

        if let Some(matching) = existing["data"].as_array().and_then(|arr| {
            arr.iter().find(|c| {
                c["metadata"]["oxy_feature"].as_str() == Some(RESTRICTED_PORTAL_FEATURE_NAME)
            })
        }) {
            return Ok(matching["id"].as_str().unwrap_or_default().to_string());
        }

        // Default fallback URL on the global config — per-session
        // `return_url` (set in `create_portal_session`) overrides this. The
        // root path goes through `PostLoginDispatcher` which finds the
        // user's last workspace.
        let return_url = format!("{}/", self.client.public_url());
        let params: Vec<(String, String)> = vec![
            (
                "metadata[oxy_feature]".into(),
                RESTRICTED_PORTAL_FEATURE_NAME.into(),
            ),
            ("default_return_url".into(), return_url),
            (
                "features[payment_method_update][enabled]".into(),
                "true".into(),
            ),
            ("features[invoice_history][enabled]".into(), "true".into()),
            ("features[customer_update][enabled]".into(), "true".into()),
            (
                "features[customer_update][allowed_updates][]".into(),
                "email".into(),
            ),
            (
                "features[customer_update][allowed_updates][]".into(),
                "phone".into(),
            ),
            (
                "features[customer_update][allowed_updates][]".into(),
                "address".into(),
            ),
            (
                "features[customer_update][allowed_updates][]".into(),
                "tax_id".into(),
            ),
            (
                "features[customer_update][allowed_updates][]".into(),
                "name".into(),
            ),
            (
                "features[subscription_cancel][enabled]".into(),
                "false".into(),
            ),
            (
                "features[subscription_update][enabled]".into(),
                "false".into(),
            ),
            (
                "business_profile[headline]".into(),
                "Manage your billing".into(),
            ),
        ];

        let idem = format!("portal-config:{RESTRICTED_PORTAL_FEATURE_NAME}");
        let created: JsonValue = self
            .client
            .form_repeated(
                Method::POST,
                "/v1/billing_portal/configurations",
                &params,
                Some(&idem),
            )
            .await?;
        Ok(created["id"].as_str().unwrap_or_default().to_string())
    }
}
