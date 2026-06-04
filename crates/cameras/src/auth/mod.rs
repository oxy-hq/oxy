//! Device-token authentication for edge boxes.
//!
//! Edge boxes authenticate to the control plane with a long-lived bearer
//! token issued at registration. The plaintext is **never** stored — only
//! its SHA-256 hash plus an 8-char prefix (safe to log for support).
//!
//! ## Lifecycle
//!
//! 1. **Issue** — at `POST /control/register`, the service generates a
//!    fresh token via [`token::issue`] and persists `(token_hash,
//!    token_prefix)` to `edge_box_tokens`. The plaintext is returned
//!    once in the response.
//! 2. **Verify** — every subsequent `/control/*` request carries
//!    `Authorization: Bearer <plaintext>`. The middleware hashes the
//!    inbound bearer and looks up the row by `token_hash`, rejecting if
//!    not found or `revoked_at IS NOT NULL`.
//! 3. **Revoke** — operators set `revoked_at`. The row stays around for
//!    audit; cleanup is a periodic sweep (separate from the auth path).
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
pub mod token;

pub use context::EdgeContext;
pub use extractor::EdgeContextExtractor;
pub use middleware::require_device_token;
pub use token::{IssuedToken, hash, issue};
