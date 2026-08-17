//! Adapters that re-implement airway's `SourceConnector` and
//! `Destination` traits on `Box<dyn …>` so factory-returned trait
//! objects compose into the `impl Trait + 'static` bounds airway's
//! constructors require.
//!
//! Without these, `Source::try_from_connector_with(impl SourceConnector +
//! 'static, …)` and `Pipeline::new(name, impl Destination + 'static)` can't accept the
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
use airway::normalizer::relational::{KeyPropagation, NormalizedOutput};
use airway::source::resource::{RecordStream, ResourceStateHandle};
use airway::types::RecordBatch;
use airway::{AirwayError, Schema};
use async_trait::async_trait;
use std::collections::HashMap;

/// Newtype wrapping a `Box<dyn SourceConnector>` so it satisfies the
/// `impl SourceConnector + 'static` bound on
/// [`airway::Source::try_from_connector_with`].
///
/// Named for the constructor oxy actually calls. The infallible
/// `Source::from_connector` carries the same bound and still exists upstream,
/// but `run_pipeline` deliberately never uses it — it is `-> Self` and so
/// cannot refuse, which is what left the admission policies dark before
/// 0.1.23.
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

    // The remaining `SourceConnector` methods are all trait-defaulted, so —
    // exactly like `extract_stream` above — the box silently substitutes the
    // default unless we forward. `partition_keys`/`sort_keys`/`key_propagation`
    // carry the DuckLake partition + sort + ancestor-key hints a connector
    // declares (e.g. Toast's `orders`: partition by restaurant_guid +
    // year(business_date), sort by business_date); dropping them leaves
    // `Table.partition_by`/`sort_by` empty so the destination's
    // `SET PARTITIONED BY`/`SET SORTED BY` never fire. `table_name_mappings`
    // renames tables, and `extract_all` is overridden by Toast/rest_api — the
    // box's default would bypass those. Every defaulted method MUST be forwarded.
    fn table_name_mappings(&self) -> HashMap<String, String> {
        self.0.table_name_mappings()
    }

    fn key_propagation(&self) -> KeyPropagation {
        self.0.key_propagation()
    }

    fn partition_keys(&self) -> HashMap<String, Vec<String>> {
        self.0.partition_keys()
    }

    fn sort_keys(&self) -> HashMap<String, Vec<String>> {
        self.0.sort_keys()
    }

    fn excluded_tables(&self) -> Vec<String> {
        self.0.excluded_tables()
    }

    // 0.1.30's addition, defaulted and therefore masked like the rest.
    //
    // Worse than most, because the failure is invisible from here: unforwarded,
    // the box returns an empty map and every declared column type silently
    // reverts to *inferred*. Inference only sees non-null values, so a column
    // that is entirely null in one file does not materialize at all — and the
    // landed table's shape then depends on which file loaded first. UberEats
    // ships reports with `store_id` blank for a whole store, which is the case
    // the seam exists for, so this is not a hypothetical.
    //
    // Nothing about that fails a compile or a run: the pipeline succeeds, the
    // table is just the wrong shape.
    fn column_hints(&self) -> HashMap<String, HashMap<String, airway::types::ColumnHints>> {
        self.0.column_hints()
    }

    // 0.1.23's four additions, all defaulted and therefore all subject to
    // the same masking as `extract_stream` above. `contracts` and
    // `sandbox_base_url` are what `connector::admit_with` reads: unforwarded,
    // `require_declared` refuses every cursored resource (the map reads
    // empty) and `environment = sandbox` refuses every connector (no host
    // declared) — both silently, and both the opposite of the truth.
    // `contract_for` and `check_contracts` default to deriving from
    // `contracts()`, so they are correct once it is forwarded; they are
    // forwarded anyway so a connector that overrides them is not bypassed.
    // `contract_for` is not belt-and-braces, either: `Source::build` calls
    // it directly for every resource on every run, regardless of contract
    // policy, to derive that resource's `version_column` — an unforwarded
    // `contract_for` silently disarms the destination's version-guarded
    // writes, not just admission.
    fn contracts(&self) -> HashMap<String, airway::connector::SourceContract> {
        self.0.contracts()
    }

    fn sandbox_base_url(&self) -> Option<&str> {
        self.0.sandbox_base_url()
    }

    fn contract_for(&self, resource: &str) -> airway::connector::SourceContract {
        self.0.contract_for(resource)
    }

    fn check_contracts(&self) -> Result<(), AirwayError> {
        self.0.check_contracts()
    }

    async fn extract_all(
        &self,
        states: &HashMap<String, serde_json::Value>,
    ) -> Result<HashMap<String, ExtractionResult>, AirwayError> {
        self.0.extract_all(states).await
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

    // Defaulted, same as the pair above: the trait default returns
    // `Err("drop_tables is not supported by this destination")`, so an
    // unforwarded call would break the host-side "reset schema" flow
    // (drop tables → clear stored schema → re-run) against a destination
    // that fully supports dropping them.
    async fn drop_tables(&self, tables: &[String]) -> Result<(), AirwayError> {
        self.0.drop_tables(tables).await
    }

    fn describe(&self) -> String {
        self.0.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airway::connector::{ContractPolicy, Environment, SourceContract, admit_with};
    use airway::types::WriteDisposition;
    use std::sync::Arc;

    const SANDBOX: &str = "https://sandbox.example.invalid";

    /// A connector that declares both a contract and a sandbox host, so a
    /// dropped forward shows up as the trait default rather than as a
    /// compile error.
    struct DeclaringConnector;

    #[async_trait]
    impl SourceConnector for DeclaringConnector {
        fn name(&self) -> &str {
            "declaring"
        }

        fn resources(&self) -> Vec<ResourceInfo> {
            vec![ResourceInfo {
                name: "orders".to_string(),
                description: None,
                write_disposition: WriteDisposition::Merge,
                primary_key: Some(vec!["guid".to_string()]),
                // Cursored: `ContractPolicy::check` skips resources without
                // a cursor, so an uncursored fixture would pass vacuously.
                cursor_field: Some("modifiedDate".to_string()),
            }]
        }

        fn column_hints(&self) -> HashMap<String, HashMap<String, airway::types::ColumnHints>> {
            HashMap::from([(
                "orders".to_string(),
                HashMap::from([(
                    "total".to_string(),
                    airway::types::ColumnHints {
                        data_type: Some(airway::types::DataType::Double),
                        ..Default::default()
                    },
                )]),
            )])
        }

        fn contracts(&self) -> HashMap<String, SourceContract> {
            HashMap::from([("orders".to_string(), SourceContract::immutable())])
        }

        fn sandbox_base_url(&self) -> Option<&str> {
            Some(SANDBOX)
        }

        async fn extract(
            &self,
            _resource: &str,
            _state: Option<&serde_json::Value>,
        ) -> Result<ExtractionResult, AirwayError> {
            unimplemented!("not exercised by these tests")
        }
    }

    // ── BoxedDestination ────────────────────────────────────────────────
    //
    // The destination wrapper is the half that shipped broken: `drop_tables`
    // was added upstream in 0.1.21 and went unforwarded until this branch, so
    // reset-schema hit the trait default — `Err("drop_tables is not supported
    // by this destination")` — against destinations that fully support it.
    // Latent only because oxy's one caller holds the unboxed handle.

    /// Records the tables it was asked to drop into a handle the test still
    /// owns. `BoxedDestination` takes ownership of the destination, so the
    /// recorder has to be reachable from outside the box — sharing the log
    /// rather than the destination is what lets the test assert on the real
    /// forward instead of standing up a second one to observe through.
    #[derive(Default)]
    struct RecordingDestination {
        dropped: Arc<std::sync::Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Destination for RecordingDestination {
        fn capabilities(&self) -> DestinationCapabilities {
            DestinationCapabilities {
                name: "recording".into(),
                supports_merge: true,
                supports_replace: true,
                max_identifier_length: 64,
                max_query_length: None,
                supported_file_formats: Vec::new(),
                case_sensitive: false,
            }
        }
        async fn migrate_schema(&self, _schema: &Schema) -> Result<(), AirwayError> {
            Ok(())
        }
        async fn load(
            &self,
            _data: &NormalizedOutput,
            _schema: &Schema,
            _load_id: &str,
        ) -> Result<LoadInfo, AirwayError> {
            unimplemented!("not exercised by these tests")
        }
        async fn replace(
            &self,
            _t: &str,
            _d: &RecordBatch,
            _s: &Schema,
            _l: &str,
        ) -> Result<(), AirwayError> {
            unimplemented!("not exercised by these tests")
        }
        async fn merge(
            &self,
            _t: &str,
            _d: &RecordBatch,
            _s: &Schema,
            _l: &str,
        ) -> Result<(), AirwayError> {
            unimplemented!("not exercised by these tests")
        }
        async fn drop_tables(&self, tables: &[String]) -> Result<(), AirwayError> {
            self.dropped
                .lock()
                .expect("test mutex")
                .extend_from_slice(tables);
            Ok(())
        }
        fn supports_streaming(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn box_forwards_drop_tables() {
        let dropped: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
        let boxed = BoxedDestination(Box::new(RecordingDestination {
            dropped: dropped.clone(),
        }));
        boxed
            .drop_tables(&["orders".to_string()])
            .await
            .expect("the trait default would return Err — this must reach the inner destination");
        assert_eq!(
            dropped.lock().expect("test mutex").as_slice(),
            &["orders".to_string()],
            "`drop_tables` fell through to the trait default"
        );
    }

    #[test]
    fn box_forwards_supports_streaming() {
        let boxed = BoxedDestination(Box::new(RecordingDestination::default()));
        assert!(
            boxed.supports_streaming(),
            "`supports_streaming()` fell through to the `false` trait default, which \
             silently disables the entire streaming path through the box"
        );
    }

    /// Unforwarded, this returns the empty default and every declared column
    /// type reverts to *inferred* — which only sees non-null values, so a
    /// column null throughout one file does not materialize and the table's
    /// shape follows load order. Nothing fails: the run succeeds with the wrong
    /// schema, which is why it needs a test rather than a reviewer.
    #[test]
    fn box_forwards_column_hints() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        let hints = boxed.column_hints();
        assert_eq!(
            hints.len(),
            1,
            "`column_hints()` fell through to the empty trait default"
        );
        assert_eq!(
            hints["orders"]["total"].data_type,
            Some(airway::types::DataType::Double),
            "the declared type must survive the box, not just the key"
        );
    }

    #[test]
    fn box_forwards_contracts() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        assert_eq!(
            boxed.contracts().len(),
            1,
            "`contracts()` fell through to the empty trait default"
        );
    }

    #[test]
    fn box_forwards_contract_for() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        assert_eq!(boxed.contract_for("orders"), SourceContract::immutable());
    }

    #[test]
    fn box_forwards_sandbox_base_url() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        assert_eq!(
            boxed.sandbox_base_url(),
            Some(SANDBOX),
            "`sandbox_base_url()` fell through to the `None` trait default"
        );
    }

    #[test]
    fn box_forwards_check_contracts() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        boxed
            .check_contracts()
            .expect("every declared name matches a real resource");
    }

    /// The regression that actually costs something: with `contracts()`
    /// dropped, `ContractPolicy::check` sees an empty map and refuses a
    /// connector that declares correctly — the policy reporting the exact
    /// opposite of the truth, with no error anywhere to trace it to.
    #[test]
    fn require_declared_admits_a_declaring_connector_through_the_box() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        admit_with(
            &boxed,
            ContractPolicy::RequireDeclared,
            Environment::Production,
        )
        .expect("a declaring connector must pass `require_declared` through the box");
    }

    /// Same shape, other axis: `sandbox` must not refuse a connector that
    /// declares a sandbox host just because the box hid the declaration.
    #[test]
    fn sandbox_admits_a_connector_declaring_a_host_through_the_box() {
        let boxed = BoxedSourceConnector(Box::new(DeclaringConnector));
        admit_with(&boxed, ContractPolicy::Permissive, Environment::Sandbox)
            .expect("a connector declaring a sandbox host must pass `environment = sandbox`");
    }
}
