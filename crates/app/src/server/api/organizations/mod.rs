//! Organization HTTP surface: org CRUD, member management (list / role
//! change / removal), and the invitation lifecycle (create / bulk / list /
//! revoke / accept), plus the invitation email sender.
//!
//! - [`dto`]: request/response serde types shared across handlers.
//! - [`ops`]: internal helpers — slug generation/reservation, invite-email
//!   normalization, per-org count queries, and the invitation email sender.
//! - [`org_handlers`] / [`member_handlers`] / [`invitation_handlers`]: the
//!   HTTP handler functions themselves, split by concern.

mod dto;
mod invitation_handlers;
mod member_handlers;
mod ops;
mod org_handlers;

pub use invitation_handlers::*;
pub use member_handlers::*;
pub(crate) use ops::{is_reserved_slug, send_invitation_email, slugify_name};
pub use org_handlers::*;
