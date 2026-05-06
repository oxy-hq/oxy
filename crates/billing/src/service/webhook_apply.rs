//! Stripe webhook idempotency table + per-event apply handlers.
//!
//! `lookup_event` / `upsert_processing` / `mark_terminal` form the
//! idempotency wrapper around `apply_event`, which dispatches to one of the
//! `on_*` handlers below. All apply paths are idempotent — replaying an
//! event must not double-charge or double-send.

use std::collections::BTreeMap;

use chrono::Utc;
use entity::{
    org_billing::{self, BillingStatus},
    stripe_webhook_events as wh,
};
use reqwest::Method;
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, EntityTrait, QueryFilter, QuerySelect, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::errors::BillingError;
use crate::service::BillingService;
use crate::service::dto::{BillingNotification, LookupOutcome};
use crate::service::helpers::extract::{invoice_subscription_id, pick_seat_item, time_from_unix};
use crate::service::helpers::mappers::map_stripe_status_str;
use crate::service::stripe_shapes::StripeSubscription;
use crate::state;

/// Final state for a webhook event row. `mark_terminal` writes the row in
/// either `Success` or `Failed` so the same UPDATE handles both cases.
pub(in crate::service) enum TerminalStatus {
    Success,
    Failed(String),
}

impl BillingService {
    pub async fn lookup_event(&self, event_id: &str) -> Result<LookupOutcome, BillingError> {
        let row = wh::Entity::find_by_id(event_id.to_string())
            .one(&self.db)
            .await?;
        let outcome = match row {
            Some(r) if r.status == wh::status::SUCCESS => LookupOutcome::AlreadySuccess,
            _ => LookupOutcome::NeedsApply,
        };
        Ok(outcome)
    }

    pub async fn upsert_processing(
        &self,
        event_id: &str,
        event_type: &str,
        payload: &JsonValue,
    ) -> Result<(), BillingError> {
        let now = Utc::now();
        let am = wh::ActiveModel {
            stripe_event_id: Set(event_id.to_string()),
            event_type: Set(event_type.to_string()),
            payload: Set(payload.clone()),
            processed_at: Set(now.into()),
            status: Set(wh::status::PROCESSING.to_string()),
            attempts: Set(1),
            last_error: Set(None),
        };
        let on_conflict = OnConflict::column(wh::Column::StripeEventId)
            .update_columns([
                wh::Column::EventType,
                wh::Column::Payload,
                wh::Column::ProcessedAt,
                wh::Column::Status,
                wh::Column::LastError,
            ])
            // Postgres rejects the unqualified `attempts` reference inside
            // ON CONFLICT DO UPDATE as ambiguous — the SET clause has both
            // the target row and EXCLUDED in scope. Table-qualify it.
            .value(
                wh::Column::Attempts,
                Expr::col((wh::Entity, wh::Column::Attempts)).add(1),
            )
            .to_owned();
        wh::Entity::insert(am)
            .on_conflict(on_conflict)
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub(in crate::service) async fn mark_terminal(
        &self,
        event_id: &str,
        status: TerminalStatus,
    ) -> Result<(), BillingError> {
        let (status_value, error_value) = match &status {
            TerminalStatus::Success => (wh::status::SUCCESS, None),
            TerminalStatus::Failed(msg) => (wh::status::FAILED, Some(msg.clone())),
        };
        wh::Entity::update_many()
            .filter(wh::Column::StripeEventId.eq(event_id))
            .col_expr(wh::Column::Status, Expr::value(status_value))
            .col_expr(wh::Column::LastError, Expr::value(error_value))
            .col_expr(
                wh::Column::ProcessedAt,
                Expr::value(chrono::DateTime::<chrono::FixedOffset>::from(Utc::now())),
            )
            .exec(&self.db)
            .await?;
        Ok(())
    }

    pub async fn mark_success(&self, event_id: &str) -> Result<(), BillingError> {
        self.mark_terminal(event_id, TerminalStatus::Success).await
    }

    pub async fn mark_failed(&self, event_id: &str, err: &str) -> Result<(), BillingError> {
        self.mark_terminal(event_id, TerminalStatus::Failed(err.to_string()))
            .await
    }

    pub async fn apply_event(
        &self,
        payload: &JsonValue,
    ) -> Result<Vec<BillingNotification>, BillingError> {
        let ty = payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        match ty {
            "customer.subscription.created" | "customer.subscription.updated" => {
                self.on_subscription_updated(payload).await
            }
            "customer.subscription.deleted" => {
                self.on_subscription_deleted(payload).await?;
                Ok(vec![])
            }
            "invoice.payment_action_required" => {
                self.on_payment_action_required(payload).await?;
                Ok(vec![])
            }
            "invoice.payment_failed" => {
                tracing::warn!(
                    "invoice.payment_failed received — subscription.updated handles status sync"
                );
                Ok(vec![])
            }
            "invoice.payment_succeeded" => {
                self.on_invoice_payment_succeeded(payload).await?;
                Ok(vec![])
            }
            _ => {
                tracing::debug!(event_type = %ty, "ignored stripe event");
                Ok(vec![])
            }
        }
    }

    async fn on_subscription_updated(
        &self,
        e: &JsonValue,
    ) -> Result<Vec<BillingNotification>, BillingError> {
        let sub: StripeSubscription = serde_json::from_value(e["data"]["object"].clone())
            .map_err(|err| BillingError::MalformedEvent(err.to_string()))?;
        let org_id = self
            .lookup_org_id_by_subscription(&sub)
            .await?
            .ok_or(BillingError::NoSubscription)?;
        self.apply_subscription_snapshot(org_id, &sub).await
    }

    /// Resolve `org_id` for an incoming subscription event. Tries:
    /// 1. existing `org_billing` row by `stripe_subscription_id`
    /// 2. `metadata.oxy_org_id` on the subscription itself (set on create)
    async fn lookup_org_id_by_subscription(
        &self,
        sub: &StripeSubscription,
    ) -> Result<Option<Uuid>, BillingError> {
        if let Some(row) = org_billing::Entity::find()
            .filter(org_billing::Column::StripeSubscriptionId.eq(&sub.id))
            .one(&self.db)
            .await?
        {
            return Ok(Some(row.org_id));
        }
        let full: JsonValue = self
            .client
            .get(&format!("/v1/subscriptions/{}", sub.id))
            .await?;
        Ok(full["metadata"]["oxy_org_id"]
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok()))
    }

    async fn on_subscription_deleted(&self, e: &JsonValue) -> Result<(), BillingError> {
        let sub_id = e["data"]["object"]["id"]
            .as_str()
            .ok_or_else(|| BillingError::MalformedEvent("no subscription id".into()))?;
        let row = match org_billing::Entity::find()
            .filter(org_billing::Column::StripeSubscriptionId.eq(sub_id))
            .one(&self.db)
            .await?
        {
            Some(r) => r,
            None => {
                tracing::warn!(%sub_id, "subscription.deleted for unknown sub — ignoring");
                return Ok(());
            }
        };
        let mut am: org_billing::ActiveModel = row.into();
        am.status = Set(BillingStatus::Canceled);
        am.stripe_subscription_id = Set(None);
        am.stripe_subscription_seat_item_id = Set(None);
        am.seats_paid = Set(0);
        am.payment_action_url = Set(None);
        am.updated_at = Set(Utc::now().into());
        am.update(&self.db).await?;
        Ok(())
    }

    async fn on_payment_action_required(&self, e: &JsonValue) -> Result<(), BillingError> {
        let customer_id = e["data"]["object"]["customer"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let hosted_url = e["data"]["object"]["hosted_invoice_url"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if customer_id.is_empty() || hosted_url.is_empty() {
            return Ok(());
        }
        if let Some(row) = org_billing::Entity::find()
            .filter(org_billing::Column::StripeCustomerId.eq(customer_id))
            .one(&self.db)
            .await?
        {
            let mut am: org_billing::ActiveModel = row.into();
            am.payment_action_url = Set(Some(hosted_url));
            am.updated_at = Set(Utc::now().into());
            am.update(&self.db).await?;
        }
        Ok(())
    }

    /// After the first subscription invoice is paid via the hosted invoice
    /// page, flip `collection_method` from `send_invoice` to
    /// `charge_automatically` so subsequent renewals auto-charge the saved
    /// default payment method.
    ///
    /// Only acts on `billing_reason=subscription_create` to avoid re-flipping
    /// on every renewal, and only when the customer has a default payment
    /// method (skips ACH/bank-transfer payments where no card was saved).
    /// Idempotent: no-op if subscription is already `charge_automatically`.
    async fn on_invoice_payment_succeeded(&self, e: &JsonValue) -> Result<(), BillingError> {
        let invoice = &e["data"]["object"];
        let billing_reason = invoice["billing_reason"].as_str().unwrap_or_default();
        if billing_reason != "subscription_create" {
            return Ok(());
        }

        let Some(sub_id) = invoice_subscription_id(invoice) else {
            return Ok(());
        };
        let Some(customer_id) = invoice["customer"].as_str() else {
            return Ok(());
        };

        let sub: JsonValue = self
            .client
            .get(&format!("/v1/subscriptions/{sub_id}"))
            .await?;
        if sub["collection_method"].as_str() == Some("charge_automatically") {
            return Ok(());
        }

        let customer: JsonValue = self
            .client
            .get(&format!("/v1/customers/{customer_id}"))
            .await?;
        let default_pm = customer["invoice_settings"]["default_payment_method"]
            .as_str()
            .filter(|s| !s.is_empty());
        let Some(default_pm) = default_pm else {
            tracing::info!(
                %sub_id,
                %customer_id,
                "first invoice paid but customer has no default payment method — keeping send_invoice"
            );
            return Ok(());
        };

        let mut params = BTreeMap::new();
        params.insert("collection_method".into(), "charge_automatically".into());
        let key = format!("flip-collection-method:{sub_id}");
        let _: JsonValue = self
            .client
            .form(
                Method::POST,
                &format!("/v1/subscriptions/{sub_id}"),
                &params,
                Some(&key),
            )
            .await?;
        tracing::info!(
            %sub_id,
            %customer_id,
            %default_pm,
            "flipped subscription to charge_automatically after first invoice paid"
        );
        Ok(())
    }

    /// Persist subscription state. Identifies the seat item by cached id when
    /// available, falls back to shortest-recurring-interval heuristic on the
    /// first webhook (before `provision_subscription` UPDATE has landed).
    ///
    /// `pub(super)` so the admin resync endpoint can re-apply a freshly
    /// fetched subscription when a webhook was missed.
    pub(super) async fn apply_subscription_snapshot(
        &self,
        org_id: Uuid,
        sub: &StripeSubscription,
    ) -> Result<Vec<BillingNotification>, BillingError> {
        let txn = self.db.begin().await?;
        let row = org_billing::Entity::find()
            .filter(org_billing::Column::OrgId.eq(org_id))
            .lock_exclusive()
            .one(&txn)
            .await?
            .ok_or(BillingError::OrgBillingMissing(org_id))?;

        let seat_item = pick_seat_item(&row.stripe_subscription_seat_item_id, &sub.items.data)?;
        let seat_item_id = seat_item.id.clone();
        let seats_paid = seat_item.quantity.unwrap_or(0) as i32;
        // Under `billing_mode=flexible` each item carries its own period.
        // Use the earliest period across items so the persisted
        // `current_period_end` reflects the soonest upcoming renewal.
        let period_start_unix = sub
            .items
            .data
            .iter()
            .filter_map(|i| i.current_period_start)
            .min();
        let period_end_unix = sub
            .items
            .data
            .iter()
            .filter_map(|i| i.current_period_end)
            .min();

        let new_status = map_stripe_status_str(&sub.status);
        let transition = state::apply_status_transition(state::StateInput {
            current_status: row.status,
            current_grace_ends_at: row.grace_period_ends_at.map(|t| t.with_timezone(&Utc)),
            new_status,
            event_time: Utc::now(),
        });

        let mut am: org_billing::ActiveModel = row.into();
        am.status = Set(transition.status);
        am.stripe_subscription_id = Set(Some(sub.id.clone()));
        am.stripe_subscription_seat_item_id = Set(Some(seat_item_id));
        am.seats_paid = Set(seats_paid);
        am.current_period_start = Set(period_start_unix.and_then(time_from_unix));
        am.current_period_end = Set(period_end_unix.and_then(time_from_unix));
        am.grace_period_ends_at = Set(transition.grace_ends_at.map(|t| t.into()));
        // Recovery URL is cleared whenever the sub is back to active —
        // payment_action_required webhook re-sets it if a future invoice fails.
        if transition.status == BillingStatus::Active {
            am.payment_action_url = Set(None);
        }
        am.updated_at = Set(Utc::now().into());
        am.update(&txn).await?;
        txn.commit().await?;

        let mut notifications = Vec::new();
        if transition.send_admin_email {
            if let Some(grace_ends_at) = transition.grace_ends_at {
                match self.lookup_owner(org_id).await {
                    Ok(Some((org_name, org_slug, owner_email))) => {
                        notifications.push(BillingNotification::PastDueEntered {
                            org_id,
                            org_name,
                            org_slug,
                            owner_email,
                            grace_ends_at,
                        });
                    }
                    Ok(None) => {
                        tracing::warn!(
                            ?org_id,
                            "past_due email skipped: org has no owner with an email"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(?e, ?org_id, "past_due email skipped: owner lookup failed");
                    }
                }
            }
        }
        Ok(notifications)
    }
}
