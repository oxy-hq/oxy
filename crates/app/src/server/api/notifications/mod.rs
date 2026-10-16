//! Notifications — telling a person something, and the inbox they read it in.
//!
//! # Who, not which channel
//!
//! A sender names **people** — a user, a role at a location, a team — and the
//! platform decides how each one is reached. That is the whole reason this is a
//! platform service rather than `ctx.email.send` plus `ctx.fetch`: Oxy owns the
//! identity (members, positions, frontline crew with no mailbox) and keeps the
//! addresses away from app code. Custom apps reach it through `ctx.notify`
//! ([`send::send`]); the product itself through [`platform::notify`], whose
//! notices store a kind and a subject and are rendered from Oxy's records.
//!
//! # Two halves, deliberately not one
//!
//! The **inbox** is a durable row a person opens and reads. It needs no
//! vendor, no credentials and no network, and it is written before anything
//! else is attempted.
//!
//! **Delivery** — Web Push, email, a Slack DM — is a best-effort attempt to
//! draw attention to that row, queued as a `notification_deliver` task
//! ([`task`]) and recorded per recipient and channel ([`delivery`]). A
//! notification that existed only as a push would be gone when the send
//! failed, and "we told them" would become unfalsifiable at exactly the moment
//! somebody asks whether the store was warned.
//!
//! # Where the data lives
//!
//! The inbox *signal* — who, which org, read or not — is Oxy's, in Oxy's
//! database. What the notification *says* is the org's, in the org's OLTP
//! store (`oxy_oltp::inbox`). Design and the rest of the reasoning:
//! `internal-docs/custom-app-notifications.md`.
//!
//! # Push transports
//!
//! Web Push ([`web_push`]), APNs ([`apns`]) and FCM ([`fcm`]) sit behind the
//! [`deliver::Push`] seam, each configured on its own, and [`push_routing`]
//! hands each its platform's device rows. None is handed what a notification
//! says — only its kind and inbox row id.
//!
//! # Not here
//!
//! SMS needs phone numbers the platform does not hold and carrier registration
//! no code can do.

pub mod apns;
pub mod badge;
pub mod channels;
pub mod compose;
pub mod deliver;
pub mod delivery;
pub mod fcm;
pub mod handlers;
pub mod inbox_view;
pub mod input;
pub mod links;
mod native_push;
pub mod platform;
pub mod preferences;
pub mod push_routing;
pub mod recipients;
pub mod send;
pub mod slack_dm;
pub mod task;
pub mod test_notice;
pub mod web_push;
