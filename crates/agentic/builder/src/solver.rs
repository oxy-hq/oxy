//! Builder copilot solver — agentic FSM with active `solving` and `interpreting`
//! phases.

pub(crate) mod domain_solver;
pub(crate) mod interpreting;
pub(crate) mod resuming;
pub(crate) mod solver;
pub(crate) mod solving;

pub use solver::BuilderSolver;

use std::collections::HashMap;

use agentic_core::orchestrator::{StateHandler, build_default_handlers};

use crate::{events::BuilderEvent, types::BuilderDomain};

pub fn build_builder_handlers()
-> HashMap<&'static str, StateHandler<BuilderDomain, BuilderSolver, BuilderEvent>> {
    build_default_handlers::<BuilderDomain, BuilderSolver, BuilderEvent>()
}
