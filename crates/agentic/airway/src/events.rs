//! Domain-specific events for the airway ELT pipeline.
//!
//! Mirrors `airway::airstack::PipelineEvent` 1:1 — same names, same
//! payload shapes — but defined locally so this crate owns the
//! serialization contract carried on the SSE stream. The worker
//! converts between the two via [`From`] impls when bridging engine
//! events onto the runtime event channel.

use std::collections::HashMap;

use agentic_core::events::DomainEvents;
use airway::airstack::PipelineEvent;
use airway::connector::SourceContract;
use serde::{Deserialize, Serialize};

use crate::contract::{ResourceContract, project_contracts};

/// Events emitted by an airway run.
///
/// Persisted as `(event_type, payload)` pairs in `agentic_run_events`
/// via the standard bridge. A `RowProcessor` registered in the event
/// registry deserialises them back for the SSE stream — see
/// [`crate::event_handler`].
///
/// Field shapes track [`airway::airstack::PipelineEvent`]. Add a new
/// variant here, update the `From` impl, and the worker forwards it
/// without further plumbing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum AirwayEvent {
    /// Pipeline run started — emitted before any extract/normalize/load work.
    LoadStarted {
        pipeline_name: String,
        load_id: String,
    },

    /// Full run topology (every source resource + the destination),
    /// emitted right after `LoadStarted` so the UI can render the
    /// lineage skeleton immediately.
    PipelinePlan {
        pipeline_name: String,
        load_id: String,
        resources: Vec<String>,
        destination: String,
        /// Per-resource [`SourceContract`] projection — how each resource
        /// behaves (immutable / versioned / opaque / undeclared), plus its
        /// version column and restatement window.
        ///
        /// The **only** field on this taxonomy that does not come from the
        /// engine event: airway's `PipelinePlan` carries names only, and the
        /// contracts are read off the connector oxy built. See
        /// [`AirwayEvent::from_engine`].
        ///
        /// `#[serde(default)]` because runs recorded before this field existed
        /// are replayed from `agentic_run_events` verbatim. **Empty is not
        /// "everything undeclared"** — it is "this stream carried no contract
        /// information at all", and the UI must render nothing rather than
        /// claim a fact about a run it cannot see. A run that *does* carry
        /// contracts has one entry per planned resource, undeclared ones
        /// included.
        #[serde(default)]
        contracts: Vec<ResourceContract>,
    },

    /// A resource is about to be extracted. Emitted up-front for every
    /// resource so the UI fills immediately instead of a silent gap
    /// before the first `ExtractCompleted`.
    ExtractStarted {
        pipeline_name: String,
        load_id: String,
        table: String,
    },

    /// Mid-extract progress for a resource (streaming path), rows
    /// pulled so far. Throttled; only ticks for multi-batch connectors.
    ExtractProgress {
        pipeline_name: String,
        load_id: String,
        table: String,
        rows_so_far: usize,
    },

    /// A resource finished extracting into memory. Emitted once per
    /// non-empty resource in `run_source`.
    ExtractCompleted {
        pipeline_name: String,
        load_id: String,
        table: String,
        rows_extracted: usize,
    },

    /// A table is about to be normalized. Emitted before normalize work
    /// so the normalize phase shows per-table motion.
    NormalizeStarted {
        pipeline_name: String,
        load_id: String,
        table: String,
    },

    /// A table finished the normalize phase, including any child
    /// tables produced by relational normalization for this parent.
    NormalizeCompleted {
        pipeline_name: String,
        load_id: String,
        table: String,
        rows_normalized: usize,
        child_tables: Vec<String>,
    },

    /// Destination migrate + load about to start. Useful as a phase
    /// transition marker for progress UIs.
    DestinationLoadStarted {
        pipeline_name: String,
        load_id: String,
        tables: Vec<String>,
    },

    /// A single table's load is about to begin (per-table load motion).
    TableLoadStarted {
        pipeline_name: String,
        load_id: String,
        table: String,
    },

    /// Mid-load progress for a table (streaming path), throttled.
    LoadProgress {
        pipeline_name: String,
        load_id: String,
        table: String,
        rows_written: usize,
    },

    /// A table failed to load and was skipped (streaming path). The
    /// run still completes — surfaced as "completed with errors",
    /// mirroring `ResourceFailed`.
    TableLoadFailed {
        pipeline_name: String,
        load_id: String,
        table: String,
        error: String,
    },

    /// A single table finished loading, with its destination row count.
    TableLoaded {
        pipeline_name: String,
        load_id: String,
        table: String,
        rows: usize,
    },

    /// Pipeline load completed. Emitted even when the end-of-load fold
    /// failed — every row was written, so the counts are still meaningful —
    /// but `folds` then carries the failure and the run reports an error.
    LoadCompleted {
        pipeline_name: String,
        load_id: String,
        tables: Vec<String>,
        /// Per-table row counts written to the destination.
        rows_loaded: std::collections::HashMap<String, usize>,
        duration_ms: i64,
        /// Per-table end-of-load fold outcomes, empty for destinations with
        /// no fold step (everything except airhouse `replacing`). This is
        /// what distinguishes "written" from "queryable": a failed fold
        /// leaves rows durable in the staging schema but invisible in the
        /// public one.
        ///
        /// Carried as JSON for the same reason as `SchemaEvolved::changes`
        /// — `airway::destination::TableFold` is the engine's type, and we
        /// don't want oxy's serialised payloads to drift if airway evolves
        /// it.
        #[serde(default)]
        folds: serde_json::Value,
    },

    /// Schema evolved (new tables or columns). `changes` is carried as
    /// JSON to avoid pulling `airway::schema::evolution::SchemaChange`
    /// into the serde contract — the engine type's shape changes are
    /// internal to airway, and we don't want oxy's serialised event
    /// payloads to drift if airway evolves the struct.
    SchemaEvolved {
        pipeline_name: String,
        changes: serde_json::Value,
    },

    /// A single resource failed to extract and was skipped. The run
    /// continues and still finishes with `LoadCompleted`; a run
    /// carrying any `ResourceFailed` is surfaced as
    /// "completed with errors" rather than a hard failure.
    ResourceFailed {
        pipeline_name: String,
        load_id: String,
        table: String,
        error: String,
    },

    /// Pipeline error occurred.
    PipelineError {
        pipeline_name: String,
        load_id: Option<String>,
        error: String,
    },

    /// Pipeline was cancelled mid-run via [`tokio_util::sync::CancellationToken`].
    Cancelled {
        pipeline_name: String,
        load_id: String,
    },
}

impl DomainEvents for AirwayEvent {}

impl From<PipelineEvent> for AirwayEvent {
    /// Structural bridge with **no connector in scope**, so a `PipelinePlan`
    /// built this way carries an empty `contracts` list — "nothing known".
    ///
    /// Every caller that *does* hold the connector must use
    /// [`AirwayEvent::from_engine`] instead; the worker's `EventForwarder`
    /// does. Routing both through one function keeps "we were never given a
    /// connector" a deliberate argument rather than a forgotten field.
    fn from(event: PipelineEvent) -> Self {
        Self::from_engine(event, None)
    }
}

impl AirwayEvent {
    /// Convert an engine event, projecting the connector's **declared**
    /// contract map onto [`AirwayEvent::PipelinePlan`].
    ///
    /// `declared` is `SourceConnector::contracts()` — never `contract_for()`,
    /// which substitutes `SourceContract::default()` (opaque) and would
    /// silently promote "nobody said" to a checked vendor fact. See
    /// [`crate::contract`].
    ///
    /// `None` is **not** the same as `Some(empty map)`, and the distinction is
    /// the whole point of the option:
    /// - `Some(map)` — a connector was asked. Every planned resource gets an
    ///   entry, `undeclared` for the ones the map omits. An empty map is the
    ///   normal case for a connector that declares nothing, and every resource
    ///   is then honestly labelled `undeclared`.
    /// - `None` — no connector was in scope, so nothing may be said about any
    ///   resource. The list stays empty and the UI renders nothing rather than
    ///   reporting a connector's silence it never actually heard.
    ///
    /// Only `PipelinePlan` reads it; every other variant is a 1:1 field move.
    pub fn from_engine(
        event: PipelineEvent,
        declared: Option<&HashMap<String, SourceContract>>,
    ) -> Self {
        match event {
            PipelineEvent::LoadStarted {
                pipeline_name,
                load_id,
            } => AirwayEvent::LoadStarted {
                pipeline_name,
                load_id,
            },
            PipelineEvent::PipelinePlan {
                pipeline_name,
                load_id,
                resources,
                destination,
            } => AirwayEvent::PipelinePlan {
                pipeline_name,
                load_id,
                contracts: declared
                    .map(|declared| project_contracts(&resources, declared))
                    .unwrap_or_default(),
                resources,
                destination,
            },
            PipelineEvent::ExtractStarted {
                pipeline_name,
                load_id,
                table,
            } => AirwayEvent::ExtractStarted {
                pipeline_name,
                load_id,
                table,
            },
            PipelineEvent::ExtractProgress {
                pipeline_name,
                load_id,
                table,
                rows_so_far,
            } => AirwayEvent::ExtractProgress {
                pipeline_name,
                load_id,
                table,
                rows_so_far,
            },
            PipelineEvent::ExtractCompleted {
                pipeline_name,
                load_id,
                table,
                rows_extracted,
            } => AirwayEvent::ExtractCompleted {
                pipeline_name,
                load_id,
                table,
                rows_extracted,
            },
            PipelineEvent::NormalizeStarted {
                pipeline_name,
                load_id,
                table,
            } => AirwayEvent::NormalizeStarted {
                pipeline_name,
                load_id,
                table,
            },
            PipelineEvent::NormalizeCompleted {
                pipeline_name,
                load_id,
                table,
                rows_normalized,
                child_tables,
            } => AirwayEvent::NormalizeCompleted {
                pipeline_name,
                load_id,
                table,
                rows_normalized,
                child_tables,
            },
            PipelineEvent::DestinationLoadStarted {
                pipeline_name,
                load_id,
                tables,
            } => AirwayEvent::DestinationLoadStarted {
                pipeline_name,
                load_id,
                tables,
            },
            PipelineEvent::TableLoadStarted {
                pipeline_name,
                load_id,
                table,
            } => AirwayEvent::TableLoadStarted {
                pipeline_name,
                load_id,
                table,
            },
            PipelineEvent::LoadProgress {
                pipeline_name,
                load_id,
                table,
                rows_written,
            } => AirwayEvent::LoadProgress {
                pipeline_name,
                load_id,
                table,
                rows_written,
            },
            PipelineEvent::TableLoadFailed {
                pipeline_name,
                load_id,
                table,
                error,
            } => AirwayEvent::TableLoadFailed {
                pipeline_name,
                load_id,
                table,
                error,
            },
            PipelineEvent::TableLoaded {
                pipeline_name,
                load_id,
                table,
                rows,
            } => AirwayEvent::TableLoaded {
                pipeline_name,
                load_id,
                table,
                rows,
            },
            PipelineEvent::LoadCompleted {
                pipeline_name,
                load_id,
                tables,
                rows_loaded,
                duration_ms,
                folds,
            } => AirwayEvent::LoadCompleted {
                pipeline_name,
                load_id,
                tables,
                rows_loaded,
                duration_ms,
                // Same round-trip rationale as `SchemaEvolved` below.
                folds: serde_json::to_value(&folds).unwrap_or(serde_json::Value::Null),
            },
            PipelineEvent::SchemaEvolved {
                pipeline_name,
                changes,
            } => {
                // `SchemaChange` is JSON-serialisable; round-trip
                // through `to_value` keeps the engine struct out of
                // our public contract.
                let changes = serde_json::to_value(&changes).unwrap_or(serde_json::Value::Null);
                AirwayEvent::SchemaEvolved {
                    pipeline_name,
                    changes,
                }
            }
            PipelineEvent::ResourceFailed {
                pipeline_name,
                load_id,
                table,
                error,
            } => AirwayEvent::ResourceFailed {
                pipeline_name,
                load_id,
                table,
                error,
            },
            PipelineEvent::PipelineError {
                pipeline_name,
                load_id,
                error,
            } => AirwayEvent::PipelineError {
                pipeline_name,
                load_id,
                error,
            },
            PipelineEvent::Cancelled {
                pipeline_name,
                load_id,
            } => AirwayEvent::Cancelled {
                pipeline_name,
                load_id,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_started_round_trips() {
        let ev = AirwayEvent::LoadStarted {
            pipeline_name: "shopify_orders".to_string(),
            load_id: "load-abc".to_string(),
        };
        let json = serde_json::to_value(&ev).expect("serialize");
        let back: AirwayEvent = serde_json::from_value(json).expect("deserialize");
        match back {
            AirwayEvent::LoadStarted {
                pipeline_name,
                load_id,
            } => {
                assert_eq!(pipeline_name, "shopify_orders");
                assert_eq!(load_id, "load-abc");
            }
            _ => panic!("wrong variant after round-trip"),
        }
    }

    #[test]
    fn variants_use_snake_case_tag() {
        let ev = AirwayEvent::PipelineError {
            pipeline_name: "p1".to_string(),
            load_id: Some("l1".to_string()),
            error: "boom".to_string(),
        };
        let value = serde_json::to_value(&ev).unwrap();
        assert_eq!(value["event_type"], "pipeline_error");
    }

    #[test]
    fn from_engine_load_completed_preserves_row_counts() {
        let mut rows_loaded = std::collections::HashMap::new();
        rows_loaded.insert("orders".to_string(), 42usize);
        rows_loaded.insert("customers".to_string(), 17usize);

        let engine_event = PipelineEvent::LoadCompleted {
            pipeline_name: "shopify".to_string(),
            load_id: "load-xyz".to_string(),
            tables: vec!["orders".to_string(), "customers".to_string()],
            rows_loaded: rows_loaded.clone(),
            duration_ms: 1234,
            folds: vec![airway::destination::TableFold {
                table: "orders".to_string(),
                outcome: airway::destination::FoldOutcome::Committed {
                    watermark: "2026-07-21T00:00:00".to_string(),
                    attempts: 1,
                },
            }],
        };

        let domain_event: AirwayEvent = engine_event.into();
        match domain_event {
            AirwayEvent::LoadCompleted {
                rows_loaded: got,
                duration_ms,
                folds,
                ..
            } => {
                assert_eq!(got, rows_loaded);
                assert_eq!(duration_ms, 1234);
                // Folds cross the boundary as JSON (the `SchemaEvolved`
                // rationale): assert the outcome survives the round-trip.
                assert_eq!(folds[0]["table"], serde_json::json!("orders"));
                assert_eq!(
                    folds[0]["outcome"]["status"],
                    serde_json::json!("committed")
                );
            }
            _ => panic!("wrong variant after From"),
        }
    }

    #[test]
    fn from_engine_resource_failed_maps_and_tags_snake_case() {
        let engine_event = PipelineEvent::ResourceFailed {
            pipeline_name: "toast".to_string(),
            load_id: "load-1".to_string(),
            table: "dining_options".to_string(),
            error: "HTTP 403 Forbidden".to_string(),
        };
        let domain_event: AirwayEvent = engine_event.into();
        let value = serde_json::to_value(&domain_event).unwrap();
        assert_eq!(value["event_type"], "resource_failed");
        match domain_event {
            AirwayEvent::ResourceFailed { table, error, .. } => {
                assert_eq!(table, "dining_options");
                assert!(error.contains("403"));
            }
            _ => panic!("wrong variant after From"),
        }
    }

    #[test]
    fn from_engine_extract_progress_maps_and_tags_snake_case() {
        let ev: AirwayEvent = PipelineEvent::ExtractProgress {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
            rows_so_far: 250,
        }
        .into();
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["event_type"], "extract_progress");
        assert_eq!(v["rows_so_far"], 250);
    }

    #[test]
    fn from_engine_streaming_load_events_map_and_tag_snake_case() {
        let p: AirwayEvent = PipelineEvent::LoadProgress {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
            rows_written: 17,
        }
        .into();
        let pv = serde_json::to_value(&p).unwrap();
        assert_eq!(pv["event_type"], "load_progress");
        assert_eq!(pv["rows_written"], 17);

        let f: AirwayEvent = PipelineEvent::TableLoadFailed {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
            error: "boom".into(),
        }
        .into();
        assert_eq!(
            serde_json::to_value(&f).unwrap()["event_type"],
            "table_load_failed"
        );
    }

    #[test]
    fn from_engine_table_load_events_map_and_tag_snake_case() {
        let s: AirwayEvent = PipelineEvent::TableLoadStarted {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
        }
        .into();
        assert_eq!(
            serde_json::to_value(&s).unwrap()["event_type"],
            "table_load_started"
        );
        let d: AirwayEvent = PipelineEvent::TableLoaded {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
            rows: 42,
        }
        .into();
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["event_type"], "table_loaded");
        assert_eq!(v["rows"], 42);
    }

    fn plan(resources: &[&str]) -> PipelineEvent {
        PipelineEvent::PipelinePlan {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            resources: resources.iter().map(|s| (*s).to_string()).collect(),
            destination: "memory".into(),
        }
    }

    #[test]
    fn from_engine_pipeline_plan_maps_and_tags_snake_case() {
        let ev: AirwayEvent = plan(&["users", "orders"]).into();
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["event_type"], "pipeline_plan");
        assert_eq!(v["destination"], "memory");
        match ev {
            AirwayEvent::PipelinePlan { resources, .. } => {
                assert_eq!(resources, vec!["users", "orders"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    /// The contract-display payload: one entry per planned resource, in plan
    /// order, with the *undeclared* resource labelled rather than defaulted.
    #[test]
    fn from_engine_pipeline_plan_projects_declared_contracts() {
        let declared = HashMap::from([("orders".to_string(), SourceContract::immutable())]);
        let ev = AirwayEvent::from_engine(plan(&["users", "orders"]), Some(&declared));
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["contracts"][0]["resource"], "users");
        assert_eq!(
            v["contracts"][0]["mutability"], "undeclared",
            "an undeclared resource must not arrive wearing airway's opaque default"
        );
        assert_eq!(v["contracts"][1]["resource"], "orders");
        assert_eq!(v["contracts"][1]["mutability"], "immutable");
    }

    /// `From` has no connector, so it must say *nothing* rather than guess —
    /// an empty list, which the UI reads as "no contract info on this stream".
    ///
    /// Contrast the next test: a connector that *was* asked and declared
    /// nothing yields `undeclared` entries, which is a different claim.
    #[test]
    fn from_without_a_connector_carries_no_contracts() {
        let ev: AirwayEvent = plan(&["users"]).into();
        match ev {
            AirwayEvent::PipelinePlan { contracts, .. } => assert!(
                contracts.is_empty(),
                "no connector in scope means nothing may be asserted about any resource"
            ),
            _ => panic!("wrong variant"),
        }
    }

    /// An empty `contracts()` map is the normal case for a connector that has
    /// not been migrated to 0.1.23's contracts. It was still *asked*, so every
    /// planned resource is labelled `undeclared` — not omitted, and not
    /// defaulted to `opaque`.
    #[test]
    fn an_empty_declared_map_still_labels_every_resource_undeclared() {
        let ev = AirwayEvent::from_engine(plan(&["users", "orders"]), Some(&HashMap::new()));
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["contracts"].as_array().map(Vec::len), Some(2));
        assert_eq!(v["contracts"][0]["mutability"], "undeclared");
        assert_eq!(v["contracts"][1]["mutability"], "undeclared");
    }

    /// Runs recorded before the field existed replay verbatim out of
    /// `agentic_run_events`; they must still deserialize.
    #[test]
    fn pipeline_plan_without_contracts_field_still_deserializes() {
        let legacy = serde_json::json!({
            "event_type": "pipeline_plan",
            "pipeline_name": "p",
            "load_id": "l",
            "resources": ["users"],
            "destination": "memory",
        });
        let ev: AirwayEvent = serde_json::from_value(legacy).expect("legacy payload deserializes");
        match ev {
            AirwayEvent::PipelinePlan {
                contracts,
                resources,
                ..
            } => {
                assert_eq!(resources, vec!["users"]);
                assert!(contracts.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn from_engine_started_events_map_and_tag_snake_case() {
        let es: AirwayEvent = PipelineEvent::ExtractStarted {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
        }
        .into();
        assert_eq!(
            serde_json::to_value(&es).unwrap()["event_type"],
            "extract_started"
        );
        let ns: AirwayEvent = PipelineEvent::NormalizeStarted {
            pipeline_name: "p".into(),
            load_id: "l".into(),
            table: "orders".into(),
        }
        .into();
        assert_eq!(
            serde_json::to_value(&ns).unwrap()["event_type"],
            "normalize_started"
        );
    }

    #[test]
    fn from_engine_pipeline_error_with_no_load_id() {
        let engine_event = PipelineEvent::PipelineError {
            pipeline_name: "p1".to_string(),
            load_id: None,
            error: "config error".to_string(),
        };
        let domain_event: AirwayEvent = engine_event.into();
        match domain_event {
            AirwayEvent::PipelineError { load_id, error, .. } => {
                assert!(load_id.is_none());
                assert_eq!(error, "config error");
            }
            _ => panic!("wrong variant"),
        }
    }
}
