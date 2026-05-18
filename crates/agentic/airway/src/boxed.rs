//! Adapters that re-implement airway's `SourceConnector` and
//! `Destination` traits on `Box<dyn …>` so factory-returned trait
//! objects compose into the `impl Trait + 'static` bounds airway's
//! constructors require.
//!
//! Without these, `Source::from_connector(impl SourceConnector + 'static)`
//! and `Pipeline::new(name, impl Destination + 'static)` can't accept the
//! `Box<dyn …>` values that the [`crate::source_factory`] and
//! [`crate::destination_factory`] dispatchers return — Rust doesn't
//! auto-implement `Trait` for `Box<dyn Trait>` and airway doesn't ship a
//! blanket impl.
//!
//! Each adapter is a thin newtype that forwards every trait method to
//! the boxed value.

use airway::connector::{ExtractionResult, ResourceInfo, SourceConnector};
use airway::destination::writer::LoadWriter;
use airway::destination::{Destination, DestinationCapabilities, LoadInfo};
use airway::normalizer::relational::NormalizedOutput;
use airway::source::resource::{RecordStream, ResourceStateHandle};
use airway::types::RecordBatch;
use airway::{AirwayError, Schema};
use async_trait::async_trait;

/// Newtype wrapping a `Box<dyn SourceConnector>` so it satisfies the
/// `impl SourceConnector + 'static` bound on
/// [`airway::Source::from_connector`].
pub struct BoxedSourceConnector(pub Box<dyn SourceConnector>);

#[async_trait]
impl SourceConnector for BoxedSourceConnector {
    fn name(&self) -> &str {
        self.0.name()
    }

    fn resources(&self) -> Vec<ResourceInfo> {
        self.0.resources()
    }

    async fn extract(
        &self,
        resource: &str,
        state: Option<&serde_json::Value>,
    ) -> Result<ExtractionResult, AirwayError> {
        self.0.extract(resource, state).await
    }

    // Without this forward the trait-default `extract_stream` runs
    // (`self.extract` → one bulk batch), so a connector that overrides
    // `extract_stream` for incremental/streaming (e.g. Toast per page)
    // is silently bypassed through the box — the same masking class as
    // `BoxedDestination::{supports_streaming,streaming_writer}`. Every
    // streaming-capability trait method MUST be forwarded here.
    async fn extract_stream(
        &self,
        resource: &str,
        state: Option<&serde_json::Value>,
    ) -> Result<(RecordStream, ResourceStateHandle), AirwayError> {
        self.0.extract_stream(resource, state).await
    }
}

/// Newtype wrapping a `Box<dyn Destination>` so it satisfies the
/// `impl Destination + 'static` bound on [`airway::Pipeline::new`].
pub struct BoxedDestination(pub Box<dyn Destination>);

#[async_trait]
impl Destination for BoxedDestination {
    fn capabilities(&self) -> DestinationCapabilities {
        self.0.capabilities()
    }

    async fn migrate_schema(&self, schema: &Schema) -> Result<(), AirwayError> {
        self.0.migrate_schema(schema).await
    }

    async fn load(
        &self,
        data: &NormalizedOutput,
        schema: &Schema,
        load_id: &str,
    ) -> Result<LoadInfo, AirwayError> {
        self.0.load(data, schema, load_id).await
    }

    async fn replace(
        &self,
        table_name: &str,
        data: &RecordBatch,
        schema: &Schema,
        load_id: &str,
    ) -> Result<(), AirwayError> {
        self.0.replace(table_name, data, schema, load_id).await
    }

    async fn merge(
        &self,
        table_name: &str,
        data: &RecordBatch,
        schema: &Schema,
        load_id: &str,
    ) -> Result<(), AirwayError> {
        self.0.merge(table_name, data, schema, load_id).await
    }

    // Without these two forwards the trait defaults
    // (`supports_streaming() == false`, `streaming_writer() == None`)
    // win, so airway's `run_source` streaming gate
    // (`config.streaming && destination.supports_streaming()`) is
    // always false through the box: every oxy-driven run silently
    // falls back to the bulk extract-all → normalize-all → row-by-row
    // `load` path, and all of phases 1–4 (concurrent extract↔sink,
    // per-batch commit, ExtractProgress/LoadProgress) is dead code.
    fn supports_streaming(&self) -> bool {
        self.0.supports_streaming()
    }

    fn streaming_writer(&self, schema: &Schema, load_id: &str) -> Option<Box<dyn LoadWriter>> {
        self.0.streaming_writer(schema, load_id)
    }

    fn describe(&self) -> String {
        self.0.describe()
    }
}
