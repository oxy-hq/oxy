mod automation_templates;
mod context;
pub mod formatters;
pub mod renderer;
// `types` relocated to `crate::exec_types` (Phase 3) so the type vocabulary
// survives the deletion of this module in Phase 4. `pub(crate)` because every
// out-of-crate consumer already imports `oxy::exec_types` directly — scoping the
// shim to the crate makes the compiler enforce that (a stray external
// `oxy::execute::types` no longer resolves). The remaining in-crate `super::types`
// refs are entangled with a separate `service::formatters::types`, so they ride
// the shim until their own modules relocate.
pub(crate) use crate::exec_types as types;
pub mod writer;

pub use context::{ExecutionContext, ExecutionContextBuilder};
