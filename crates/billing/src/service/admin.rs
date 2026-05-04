//! Admin-only endpoints: queue listing, live subscription detail, and the
//! Stripe Price catalogue used by the provisioning dropdown.

use chrono::Utc;
use entity::{
    org_billing::{self, BillingStatus},
    organizations,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::dto::{
    AdminOrgRow, AdminPriceDto, AdminSubscriptionDetail, AdminSubscriptionItem,
};
use crate::service::helpers::format::{format_amount_display, format_price_label};
use crate::service::helpers::mappers::{map_latest_invoice, map_subscription_item};

impl BillingService {
    /// Admin queue: orgs filterable by status.
    pub async fn list_admin_orgs(
        &self,
        status_filter: Option<&str>,
        page: u64,
        page_size: u64,
    ) -> Result<Vec<AdminOrgRow>, BillingError> {
        let mut query = org_billing::Entity::find().find_also_related(organizations::Entity);

        if let Some(s) = status_filter {
            let parsed = match s {
                "incomplete" => BillingStatus::Incomplete,
                "active" => BillingStatus::Active,
                "past_due" => BillingStatus::PastDue,
                "unpaid" => BillingStatus::Unpaid,
                "canceled" => BillingStatus::Canceled,
                other => return Err(BillingError::InvalidStatus(other.to_string())),
            };
            query = query.filter(org_billing::Column::Status.eq(parsed));
        }

        let rows = query
            .order_by_asc(org_billing::Column::CreatedAt)
            .offset(page * page_size)
            .limit(page_size)
            .all(&self.db)
            .await?;

        let mut out = Vec::with_capacity(rows.len());
        for (billing, org) in rows {
            let Some(org) = org else { continue };
            let owner_email = self.find_owner_email(org.id).await.ok();
            out.push(AdminOrgRow {
                id: org.id,
                slug: org.slug.clone(),
                name: org.name.clone(),
                owner_email,
                status: billing.status.as_str().to_string(),
                created_at: billing.created_at.with_timezone(&Utc),
                stripe_subscription_id: billing.stripe_subscription_id.clone(),
            });
        }
        Ok(out)
    }

    /// Admin: live subscription snapshot from Stripe (always fetched fresh so
    /// the dialog reflects the dashboard, not the cached row). Items are
    /// expanded with price + product so the UI can render line names.
    pub async fn admin_subscription_detail(
        &self,
        org_id: Uuid,
    ) -> Result<AdminSubscriptionDetail, BillingError> {
        let row = self.load_billing(org_id).await?;
        let sub_id = row
            .stripe_subscription_id
            .ok_or(BillingError::NoSubscription)?;
        let sub = self.fetch_subscription_with_items(&sub_id, true).await?;

        let items: Vec<AdminSubscriptionItem> = sub["items"]["data"]
            .as_array()
            .map(|arr| arr.iter().map(map_subscription_item).collect())
            .unwrap_or_default();
        // Period at sub-level mirrors the soonest-renewing item — matches
        // what `apply_subscription_snapshot` persists to `org_billing`.
        let current_period_start = items.iter().filter_map(|i| i.current_period_start).min();
        let current_period_end = items.iter().filter_map(|i| i.current_period_end).min();

        Ok(AdminSubscriptionDetail {
            id: sub["id"].as_str().unwrap_or(&sub_id).to_string(),
            status: sub["status"].as_str().unwrap_or_default().to_string(),
            livemode: sub["livemode"].as_bool().unwrap_or(false),
            created: sub["created"].as_i64(),
            current_period_start,
            current_period_end,
            cancel_at_period_end: sub["cancel_at_period_end"].as_bool().unwrap_or(false),
            collection_method: sub["collection_method"].as_str().map(str::to_string),
            customer_id: sub["customer"].as_str().map(str::to_string),
            items,
            latest_invoice: map_latest_invoice(&sub["latest_invoice"]),
        })
    }

    /// List active recurring Prices on the Stripe account for the admin
    /// provisioning dropdown.
    pub async fn list_admin_prices(&self) -> Result<Vec<AdminPriceDto>, BillingError> {
        let raw: JsonValue = self
            .client
            .get(
                "/v1/prices?active=true&type=recurring&limit=100\
                 &expand[]=data.product&expand[]=data.tiers",
            )
            .await?;

        let data = raw["data"]
            .as_array()
            .ok_or(BillingError::MalformedStripeResponse)?;

        let mut out = Vec::with_capacity(data.len());
        for p in data {
            let id = p["id"].as_str().unwrap_or_default().to_string();
            let nickname = p["nickname"].as_str().map(str::to_string);
            let unit_amount = p["unit_amount"].as_i64().unwrap_or(0);
            let currency = p["currency"].as_str().unwrap_or("usd").to_string();
            let interval = p["recurring"]["interval"]
                .as_str()
                .unwrap_or("month")
                .to_string();
            let product_name = p["product"]["name"].as_str().map(str::to_string);
            let amount_display = format_amount_display(p, unit_amount, &currency);
            let label = format_price_label(&nickname, &amount_display, &interval);
            let billing_scheme = p["billing_scheme"]
                .as_str()
                .unwrap_or("per_unit")
                .to_string();
            out.push(AdminPriceDto {
                id,
                nickname,
                unit_amount,
                currency,
                interval,
                product_name,
                label,
                amount_display,
                billing_scheme,
            });
        }
        Ok(out)
    }
}
