//! Pure helper functions used by the operational modules.
//!
//! Split by concern: pricing rules, JSON extraction, Stripe→DTO mappers,
//! formatting. Anything reusable across `provision`, `checkout`, `admin`,
//! and `webhook` lives here so it has exactly one home.

pub(super) mod extract;
pub(super) mod format;
pub(super) mod mappers;
pub(super) mod pricing;
