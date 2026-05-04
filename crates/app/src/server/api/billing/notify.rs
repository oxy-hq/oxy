//! Dispatcher for [`BillingNotification`]s emitted by `oxy-billing`.
//! Spawns fire-and-forget email sends; errors are logged but never block the
//! caller (a Stripe webhook handler or the confirm-checkout endpoint).

use oxy_billing::service::BillingNotification;

use crate::emails::billing_past_due::{PastDueEmail, send_past_due_email};

/// Spawn one background task per notification. `public_url` is captured from
/// the `BillingService`'s `StripeConfig` so it doesn't need to be re-read
/// from env on each dispatch.
pub fn dispatch(notifications: Vec<BillingNotification>, public_url: String) {
    for notif in notifications {
        match notif {
            BillingNotification::PastDueEntered {
                org_id,
                org_name,
                org_slug,
                owner_email,
                grace_ends_at,
            } => {
                let public_url = public_url.clone();
                tokio::spawn(async move {
                    let result = send_past_due_email(PastDueEmail {
                        to_email: &owner_email,
                        org_name: &org_name,
                        org_slug: &org_slug,
                        grace_ends_at,
                        public_url: &public_url,
                    })
                    .await;
                    if let Err(e) = result {
                        tracing::error!(?e, ?org_id, "past_due email send failed");
                    }
                });
            }
        }
    }
}
