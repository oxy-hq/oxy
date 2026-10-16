//! Notification categories against a real Oxy database: a catalogue read from
//! an app's live build, mutes at a category or a whole source that delivery
//! honours (and a critical kind that ignores them), the inbox's filters and
//! `before` paging, a scoped read-all, the catalogue endpoint's tree and what
//! it costs, who may tell whom (per app and batched, one rule), what tapping an
//! action resolves to, and a function action run for real through the V8
//! isolate on the tapper's own session.
//!
//! Run with:
//! `cargo nextest run -p oxy-app --test platform -E 'test(notification_categories)'`

mod actions;
mod audience;
mod catalog;
mod delivery;
mod fixture;
mod function_action;
mod inbox;
