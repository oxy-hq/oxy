//! Airway ELT runtime — Pattern B subsystem on `agentic-runtime`.
//!
//! Wraps the external [airway] engine as a queue-driven ELT pipeline:
//! `SOURCE_TYPE` + `event_handler()` plug into the runtime's
//! `EventRegistry`; `AirwayWorker` runs a pipeline spec end-to-end;
//! `source_factory`/`destination_factory` dispatch the YAML `kind:` to
//! a concrete airway connector; and the `extension` module owns the
//! `airway_*` aggregates behind `AirwayMigrator`.
//!
//! [airway]: https://github.com/oxy-hq/airway-internal

pub mod boxed;
pub mod config;
pub mod destination_factory;
pub mod error;
pub mod events;
pub mod extension;
pub mod source_factory;
pub mod state_store;
pub mod task_spec;
pub mod worker;

pub use config::{
    AirwayPipelineSpec, DestinationConfig, DestinationRef, DestinationSpec, SourceConfig,
};
pub use destination_factory::build_destination;
pub use error::AirwayError;
pub use events::AirwayEvent;
pub use extension::AirwayMigrator;
pub use source_factory::{
    DiscoveredColumn, DiscoveredTable, build_source_connector, discover_source_tables,
};
pub use state_store::AirwayPgStateStore;
pub use worker::AirwayWorker;

/// `source_type` to register this domain under in the runtime event
/// registry. Used by the SSE layer to look up the right processor for a
/// run's events.
pub const SOURCE_TYPE: &str = "airway";

/// Build a [`DomainHandler`] for registering airway events with the
/// runtime's [`EventRegistry`].
///
/// Airway events are emitted as `(event_type, payload)` JSON pairs by
/// the worker; the processor is a passthrough that preserves both
/// fields verbatim. Same shape as `agentic-workflow`'s event handler.
///
/// [`DomainHandler`]: agentic_runtime::event_registry::DomainHandler
/// [`EventRegistry`]: agentic_runtime::event_registry::EventRegistry
pub fn event_handler() -> agentic_runtime::event_registry::DomainHandler {
    use agentic_runtime::event_registry::{DomainHandler, RowProcessor};
    use std::sync::Arc;

    let processor: RowProcessor =
        Arc::new(|event_type, payload| Some(vec![(event_type.to_string(), payload.clone())]));

    DomainHandler {
        processor,
        summary_fn: Arc::new(|_| None),
        tool_summary_fn: Arc::new(|_, _| None),
        should_accumulate: Some(Arc::new(|_| false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_type_is_airway() {
        assert_eq!(SOURCE_TYPE, "airway");
    }

    #[test]
    fn event_handler_constructs() {
        // Smoke test — confirms the handler builds without panicking.
        let _ = event_handler();
    }
}
