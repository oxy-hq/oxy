//! Port: resolve the airway admission policy for an automation's airway step.
//!
//! An `.airway.yml` step inside an automation queues the same
//! [`TaskSpec::Airway`](agentic_core::delegation::TaskSpec::Airway) a schedule
//! or a manual run does, so it must be admitted under the same
//! `airway_source_config` policy. That policy lives in a Postgres table the
//! *facade* owns: resolving it needs a `DatabaseConnection`, the run's
//! `workspace_id`, and the pipeline's `source_kind` — none of which this
//! domain crate has or may acquire.
//!
//! - `workspace_id` appears nowhere in `agentic-automation`; it is
//!   `agentic_pipeline::platform::ProjectContext::workspace_id()`.
//! - `source_kind` is not carried on the step config either — it is
//!   `source.kind` inside the referenced pipeline YAML, which must be read
//!   through the compile boundary.
//! - The merge implementation itself
//!   (`agentic_pipeline::airway_config::resolve_admission`) depends on the
//!   `entity` crate, which neither this crate nor `agentic-airway` may.
//!
//! So the domain declares the port and the facade implements it — the same
//! shape as [`crate::WorkspaceContext`], and exactly what the two dispatch
//! sites' old comments asked for ("the resolver exposed behind a port this
//! domain can call"). Nothing here imports `agentic-pipeline`, and no sibling
//! domain is imported.

use agentic_core::delegation::ResolvedAdmission;

/// Resolves the effective [`ResolvedAdmission`] for one airway pipeline.
///
/// Implemented by `agentic-pipeline` (`PipelineAirwayAdmissionResolver`) and
/// injected into [`crate::AutomationDecider`] — the queue-driven path, and the
/// only one production wires. [`crate::AutomationStepOrchestrator`] takes the
/// same injection for parity, but nothing constructs that actor in production
/// today; see its `with_airway_admission_resolver` for the standing reason.
///
/// When no resolver is injected the dispatch sites fall back to
/// [`ResolvedAdmission::default`] — both fields `None`, i.e. airway's own
/// `permissive` / `production` — which is the behaviour every non-queue caller
/// (the inline Data-App runner, unit-test fixtures) already had.
#[async_trait::async_trait]
pub trait AirwayAdmissionResolver: Send + Sync {
    /// Resolve the admission for the pipeline at `pipeline_ref`.
    ///
    /// Called **at dispatch**, before the step's queue row exists, so the
    /// queued spec records the policy the run was admitted under and a past
    /// run stays explainable after an admin edits the config.
    ///
    /// `Err` fails the step rather than falling back to the default: a
    /// silently-defaulted `permissive` is precisely the "I set the policy and
    /// my automation ignored it" failure this port exists to remove, and it is
    /// indistinguishable in the data from a deployment that never set one.
    ///
    /// **`Err` therefore means "determinate, or transient past all patience"**
    /// — implementations own the retry, not their callers. An implementation
    /// backed by I/O must distinguish a failure retrying cannot fix (the ref
    /// doesn't resolve, the YAML doesn't parse, the stored policy is
    /// malformed) from a momentary one (a reset connection, an exhausted
    /// pool), return the former immediately, and re-attempt the latter within
    /// a bounded budget before returning it. This domain has no error type
    /// richer than `String` and no queue-level retry to defer to: whatever
    /// arrives here fails the whole automation run, so a resolver that
    /// forwards a raw blip turns a hiccup into a dead run. The shipped impl
    /// (`agentic_pipeline::airway_config::PipelineAirwayAdmissionResolver`)
    /// does this; the bound it retries inside is a fraction of the decision
    /// task's 60s queue lease, since the caller is holding that lease.
    async fn resolve_for_pipeline(&self, pipeline_ref: &str) -> Result<ResolvedAdmission, String>;
}
