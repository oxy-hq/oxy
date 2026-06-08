//! Device-token authentication for edge boxes.
//!
//! Every `/control/*` request carries a per-device JWT signed with the
//! device's HMAC secret (stored sealed in `device_registry`). The
//! middleware verifies the signature, resolves the active
//! `device_claims` row, and injects an [`EdgeContext`] into
//! `request.extensions` for downstream handlers.
//!
//! ## What gets injected
//!
//! On successful auth, the middleware attaches an [`EdgeContext`] to
//! `request.extensions`. Route handlers pull it back out via the
//! [`EdgeContextExtractor`] (mirrors `oxy-auth::extractor`).

pub mod context;
pub mod extractor;
pub mod jwt;
pub mod middleware;

pub use context::EdgeContext;
pub use extractor::EdgeContextExtractor;
pub use middleware::require_device_token;
