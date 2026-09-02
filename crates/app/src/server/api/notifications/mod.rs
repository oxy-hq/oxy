//! Notifications — the inbox, and the seam push delivery plugs into.
//!
//! # Two halves, deliberately not one
//!
//! The **inbox** is a durable row a person opens and reads. It is the product
//! surface, and it needs no vendor, no credentials and no network.
//!
//! **Push** is a best-effort attempt to draw attention to one of those rows.
//! It is external, failable, and — today — not configured.
//!
//! Building the inbox first is not a shortcut. A notification that exists only
//! as a push is gone when the send fails, the device is offline, or the user
//! reinstalls, and "we told them" becomes unfalsifiable at exactly the moment
//! somebody is asking whether the store was warned. Every surface that needs to
//! chase somebody — an overdue task, an announcement's read receipts — reads
//! the inbox, and gets more reliable rather than less when push lands.
//!
//! # What is NOT here
//!
//! Actual APNs / FCM / Web Push delivery. That needs Apple and Google
//! credentials this cannot be built or tested against, so what ships instead is
//! the seam: [`deliver::Push`], with a logging implementation registered by
//! default. Adding a real adapter is one impl and one registration, and nothing
//! above this line changes.
//!
//! Shipping a half-working sender would be worse than shipping none: it would
//! log successes for pushes nobody received.

pub mod deliver;
pub mod handlers;
