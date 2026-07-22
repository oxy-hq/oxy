//! Onboarding HTTP surface: workspace creation (demo / new / GitHub import),
//! LLM-key and warehouse-credential readiness checks, onboarding reset
//! ("start over"), and warehouse data-file uploads.
//!
//! - [`dto`]: request/response serde types shared across handlers.
//! - [`ops`]: internal database, filesystem, and multipart helpers plus the
//!   upload size constants.
//! - [`handlers`]: the HTTP handler functions themselves.

mod dto;
mod handlers;
mod ops;

pub use handlers::*;
pub use ops::MAX_UPLOAD_BODY_BYTES;
