//! Public webhook receivers. Mounted under the unauthenticated public
//! router because external services (Toast, Stripe, …) can't carry a user
//! JWT.

pub mod app_function;
pub mod toast;
