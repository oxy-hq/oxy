//! Authentication HTTP surface: magic-link request/verify, OAuth login
//! (Google / Okta / GitHub), OAuth-state CSRF tokens, session-cookie
//! hydration, the public auth-config endpoint, and `return_to` redirect
//! validation.
//!
//! - [`dto`]: request/response serde types shared across handlers.
//! - [`ops`]: internal helpers — session-cookie construction, OAuth code
//!   exchange, magic-link email + rate limiting, and login finalization.
//! - [`handlers`]: the HTTP handler functions themselves.

mod dto;
mod handlers;
mod ops;

pub use handlers::*;
pub(crate) use ops::{clear_session_cookie, extract_base_url_from_headers};
