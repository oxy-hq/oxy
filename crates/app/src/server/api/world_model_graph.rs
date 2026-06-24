//! World Model graph endpoints for the IDE:
//!
//! The entity-centric world model (every primary entity in the semantic
//! layer, its measures, and promotion edges) plus instance drill-down
//! (instance picker, filter counts, instance detail, measure breakdown).
//!
//! Split out of `semantic.rs` (file-size guideline): these handlers share
//! the semantic-layer load + query-execution path with the semantic
//! endpoints but form a self-contained surface. Distinct from
//! `world_model.rs`, which serves the live world-model *app* (cameras,
//! weather, event SSE, LLM proxy).

use airlayer::engine::promotions::Promotions;
use airlayer::schema::models::{AdditivityClass, EntityType, MeasureType};
use axum::{
    extract::{self, Path},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::{FuturesUnordered, StreamExt as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

use agentic_semantic::compile::{CompiledQuery, resolve_and_compile};
use agentic_semantic::config::{
    ScalarFilter, SemanticFilter, SemanticFilterType, SemanticOrder, SemanticQueryConfig,
};
use oxy_auth::extractor::AuthenticatedUserExtractor;

use super::semantic::{ErrorResponse, WorkspacePath};
use crate::server::api::data::{
    SQLParams, SemanticQueryResponse, build_connector, run_via_agentic_connector,
    run_with_connector,
};
use crate::server::api::middlewares::workspace_context::{
    EffectiveWorkspaceRole, SemanticEngineCacheCtx, SemanticLayerCacheCtx,
    WorkspaceManagerExtractor,
};
use oxy::utils::create_sse_stream;

#[derive(Deserialize)]
pub struct WmInstancesQuery {
    pub entity: String,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}
fn default_limit() -> usize {
    50
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmInstanceItem {
    pub key: String,
    pub display: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmInstancesResponse {
    pub total: usize,
    /// True when there are more records beyond the returned page (total hit the limit).
    pub has_more: bool,
    pub items: Vec<WmInstanceItem>,
}

#[derive(Deserialize)]
pub struct WmFilterCountsRequest {
    pub entity_id: String,
    pub key_value: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmEntityCount {
    pub matched: u64,
    pub total: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmFilterCountsResponse {
    pub counts: HashMap<String, WmEntityCount>,
}

/// Individual event emitted by the streaming filter-counts SSE endpoint.
/// Each event carries either a `total` count, a `matched` count, or a
/// `done: true` sentinel that marks stream completion.  The frontend
/// merges events into a running `Record<entityName, WmEntityCount>`.
#[derive(Serialize)]
pub struct WmFilterCountEvent {
    pub entity_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<u64>,
    pub done: bool,
}

#[derive(Deserialize)]
pub struct WmInstanceDetailQuery {
    pub entity: String,
    pub key: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmAttrValue {
    pub name: String,
    pub value: String,
    /// Display label from `.world-model.yml`. Falls back to `name` in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmParentRef {
    pub promotion: String,
    pub key: String,
    pub display: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmChildSample {
    pub promotion: String,
    pub fiber_count: u64,
    /// Display labels for each sample row (human-readable, may join composite PK parts).
    pub sample: Vec<String>,
    /// Canonical navigation key for each sample row — passed back as `key` to the
    /// instance-detail endpoint.  Single-PK: plain value.  Composite-PK: JSON array
    /// of the individual column values (e.g. `["70978","177411"]`).
    pub sample_keys: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmComputedMeasure {
    pub name: String,
    pub measure_type: String,
    pub value: String,
    pub fiber_count: u64,
    /// Display label from `.world-model.yml`. Falls back to `name` in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct WmMeasureName {
    pub name: String,
    pub measure_type: String,
    /// Display label from `.world-model.yml`. Falls back to `name` in the UI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WmInstanceDetailResponse {
    pub entity_id: String,
    pub key_value: String,
    pub display: String,
    pub attributes: Vec<WmAttrValue>,
    pub promotes_to: Vec<WmParentRef>,
    pub receives_from: Vec<WmChildSample>,
    pub computed_measures: Vec<WmComputedMeasure>,
}

/// Streaming event emitted by `GET /semantic/world-model/instance-detail`.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WmInstanceDetailEvent {
    /// First event: entity attributes.  Emitted as soon as the attrs query returns.
    Init {
        entity_id: String,
        key_value: String,
        display: String,
        attributes: Vec<WmAttrValue>,
    },
    /// Parent promotion(s) — emitted after Phase 2 parent lookup.
    Parent { promotes_to: Vec<WmParentRef> },
    /// One child sample — streamed individually as each child query completes.
    Child { child: WmChildSample },
    /// Emitted immediately before any measure queries fire — lists all measure names and
    /// types derived from schema alone (no DB round-trip). Lets the frontend show skeletons.
    MeasureNames { measure_names: Vec<WmMeasureName> },
    /// One batch of computed measures — emitted once per completed query group
    /// (own measures = 1 event, each induced source-view group = 1 event).
    Measure {
        computed_measures: Vec<WmComputedMeasure>,
    },
    /// Terminal event.
    Done,
}

// ── World Model ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WmMeasure {
    pub name: String,
    pub measure_type: MeasureType,
    pub additivity: AdditivityClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Raw SQL expression for the measure (e.g. "amount", "1" for count).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    /// True when this measure decomposes into component children in the metric
    /// tree (a composite `type: number` measure). Drives the driver-tree
    /// expand affordance — leaf measures have no breakdown.
    pub has_breakdown: bool,
}

#[derive(Serialize)]
pub struct WmInducedMeasure {
    pub name: String,
    pub measure_type: MeasureType,
    pub additivity: AdditivityClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub promoted_from: String,
    /// Raw SQL expression of the source measure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    /// Ordered entity names walked from source grain to this entity's grain.
    pub path: Vec<String>,
}

#[derive(Serialize)]
pub struct WmDimension {
    pub name: String,
    pub dim_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Serialize)]
pub struct WmEntity {
    pub id: String,
    pub label: String,
    pub view: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_field: Option<String>,
    pub dimensions: Vec<WmDimension>,
    pub own_measures: Vec<WmMeasure>,
    pub induced_measures: Vec<WmInducedMeasure>,
}

#[derive(Serialize)]
pub struct WmEdge {
    pub from: String,
    pub to: String,
    pub functional: bool,
}

#[derive(Serialize)]
pub struct WorldModelResponse {
    pub entities: Vec<WmEntity>,
    pub edges: Vec<WmEdge>,
}

// ── World Model — SQL helpers ─────────────────────────────────────────────────

/// Find the view where `entity_name` is declared as Primary.
fn primary_view_of<'a>(
    layer: &'a airlayer::SemanticLayer,
    entity_name: &str,
) -> Option<&'a airlayer::View> {
    layer.views.iter().find(|v| {
        v.entities
            .iter()
            .any(|e| e.entity_type == EntityType::Primary && e.name == entity_name)
    })
}

/// Get key columns for `entity_name` in `view`.
/// `is_primary`: true = Primary declaration, false = Foreign declaration.
/// Returns logical dimension names (use `entity_key_exprs_in_view` for SQL).
fn entity_keys_in_view(view: &airlayer::View, entity_name: &str, is_primary: bool) -> Vec<String> {
    view.entities
        .iter()
        .find(|e| {
            e.name == entity_name
                && if is_primary {
                    e.entity_type == EntityType::Primary
                } else {
                    e.entity_type == EntityType::Foreign
                }
        })
        .map(|e| e.get_keys())
        .unwrap_or_default()
}

/// Build per-column `IN` filters for a child entity's foreign key from the
/// matched PK rows of its parent.
///
/// `fk_dim_refs[col]` is matched positionally against column `col` of every
/// row in `parent_pk_rows`. A column is emitted only when at least one parent
/// row actually supplies a value for it.
///
/// The skip matters when the parent identifies its instances by fewer columns
/// than the child's composite FK — e.g. an `order_item` seed coming from the
/// instance picker carries only its first PK column (`order_id`), while the
/// `shipment` child references `order_item` by the composite (`order_id`,
/// `line_item_id`). Emitting a filter for the missing `line_item_id` column
/// would produce an empty `IN ()` that matches nothing, zeroing the reachable
/// count. Skipping it filters on what the seed actually constrains, matching
/// the instance-detail child count (which filters on the first FK column only).
fn child_fk_filters(
    fk_dim_refs: &[String],
    parent_pk_rows: &[Vec<serde_json::Value>],
) -> Vec<agentic_semantic::config::SemanticFilter> {
    fk_dim_refs
        .iter()
        .enumerate()
        .filter_map(|(col, fk_ref)| {
            let values: Vec<serde_json::Value> = parent_pk_rows
                .iter()
                .filter_map(|row| row.get(col).cloned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            if values.is_empty() {
                return None;
            }
            Some(agentic_semantic::config::SemanticFilter {
                field: fk_ref.clone(),
                filter_type: agentic_semantic::config::SemanticFilterType::In(
                    agentic_semantic::config::ArrayFilter { values },
                ),
            })
        })
        .collect()
}

/// Centralises the "what to SELECT and how to render a display string" logic for an entity.
///
/// Rule: if `label: <dim>` is declared, use that dimension; otherwise join all PK
/// columns with " · ".  Used by the instance picker, instance detail (init, parent,
/// child samples) so the behaviour is identical everywhere.
struct EntityDisplaySpec {
    /// View-qualified dimension refs to SELECT (all PKs first, then label dim if any).
    dims: Vec<String>,
    /// Number of leading PK columns in `dims`.
    pk_count: usize,
    /// Whether a label dim was appended after the PK columns.
    has_label_dim: bool,
    /// Unqualified label dim name (for attr-map lookup).
    label_name: Option<String>,
    /// Unqualified PK col names (for attr-map and search expressions).
    pk_names: Vec<String>,
}

impl EntityDisplaySpec {
    fn for_entity(view: &airlayer::View, entity_name: &str, display_field: Option<&str>) -> Self {
        let entity = view
            .entities
            .iter()
            .find(|e| e.name == entity_name && e.entity_type == EntityType::Primary);
        let pk_names = entity.map(|e| e.get_keys()).unwrap_or_default();
        let label_name: Option<String> = display_field.map(|s| s.to_string());
        let mut dims: Vec<String> = pk_names
            .iter()
            .map(|k| format!("{}.{}", view.name, k))
            .collect();
        let pk_count = dims.len();
        let has_label_dim = if let Some(ref lbl) = label_name {
            let lbl_ref = format!("{}.{}", view.name, lbl);
            if !dims.contains(&lbl_ref) {
                dims.push(lbl_ref);
                true
            } else {
                false
            }
        } else {
            false
        };
        Self {
            dims,
            pk_count,
            has_label_dim,
            label_name,
            pk_names,
        }
    }

    /// Build a display string from a SELECT result row (columns ordered as `self.dims`).
    fn display_from_row(&self, row: &[String]) -> String {
        if self.has_label_dim {
            let v = row.get(self.pk_count).cloned().unwrap_or_default();
            if !v.is_empty() {
                return v;
            }
        }
        self.join_pks_from_row(row)
    }

    /// Build a display string from attribute name→value pairs (e.g. the attrs query).
    fn display_from_attrs(&self, attrs: &[(String, String)]) -> String {
        if let Some(ref lbl) = self.label_name {
            if let Some((_, v)) = attrs.iter().find(|(n, _)| n == lbl) {
                if !v.is_empty() {
                    return v.clone();
                }
            }
        }
        let parts: Vec<&str> = self
            .pk_names
            .iter()
            .filter_map(|pk| attrs.iter().find(|(n, _)| n == pk).map(|(_, v)| v.as_str()))
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            String::new()
        } else {
            parts.join(" · ")
        }
    }

    fn join_pks_from_row(&self, row: &[String]) -> String {
        let parts: Vec<&str> = row[..self.pk_count.min(row.len())]
            .iter()
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        if parts.is_empty() {
            row.first().cloned().unwrap_or_default()
        } else {
            parts.join(" · ")
        }
    }
}

/// Find the entity in `seed`'s ancestry that has `ancestor` as its direct parent.
fn child_of_toward(
    promotions: &airlayer::engine::promotions::Promotions,
    seed: &str,
    ancestor: &str,
) -> Option<String> {
    let path = promotions.ancestry(seed);
    path.into_iter()
        .find(|a| promotions.parent_of(a) == Some(ancestor))
}

/// Quote a table name for SQL.
fn sql_quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Return the injected `_row_count` measure reference for a view.
fn count_measure_ref(view: &airlayer::View) -> String {
    format!("{}.{}", view.name, "__oxy_row_count")
}

fn measure_agg(measure_type: MeasureType, expr: &str) -> Option<String> {
    match measure_type {
        MeasureType::Sum => Some(format!("SUM({expr})")),
        MeasureType::Count => Some("COUNT(*)".to_string()),
        MeasureType::Average => Some(format!("AVG({expr})")),
        MeasureType::Min => Some(format!("MIN({expr})")),
        MeasureType::Max => Some(format!("MAX({expr})")),
        MeasureType::CountDistinct => Some(format!("COUNT(DISTINCT {expr})")),
        _ => None,
    }
}

fn apply_world_model_config(
    entities: &mut Vec<WmEntity>,
    edges: &mut Vec<WmEdge>,
    cfg: &super::world_model_config::WorldModelConfig,
) {
    use super::world_model_config::{WmEntityConfig, WmFieldConfig};
    use std::collections::{HashMap, HashSet};

    let entity_map: HashMap<&str, &WmEntityConfig> =
        cfg.entities.iter().map(|e| (e.id.as_str(), e)).collect();

    // Filter entities to only those listed in config
    entities.retain(|e| entity_map.contains_key(e.id.as_str()));

    // Filter edges — keep only edges where both endpoints survived the entity filter
    let kept: HashSet<&str> = entities.iter().map(|e| e.id.as_str()).collect();
    edges.retain(|e| kept.contains(e.from.as_str()) && kept.contains(e.to.as_str()));

    for entity in entities.iter_mut() {
        let Some(ec) = entity_map.get(entity.id.as_str()) else {
            continue;
        };

        if let Some(lbl) = &ec.label {
            entity.label = lbl.clone();
        }
        if let Some(desc) = &ec.description {
            entity.description = Some(desc.clone());
        }
        entity.display_field = ec.display_field.clone();

        // Filter and relabel dimensions when the config lists them explicitly
        if let Some(dim_cfgs) = &ec.dimensions {
            let dim_map: HashMap<&str, &WmFieldConfig> =
                dim_cfgs.iter().map(|d| (d.name.as_str(), d)).collect();
            let order: HashMap<&str, usize> = dim_cfgs
                .iter()
                .enumerate()
                .map(|(i, d)| (d.name.as_str(), i))
                .collect();
            entity
                .dimensions
                .retain(|d| dim_map.contains_key(d.name.as_str()));
            entity
                .dimensions
                .sort_by_key(|d| order.get(d.name.as_str()).copied().unwrap_or(usize::MAX));
            for dim in entity.dimensions.iter_mut() {
                if let Some(dc) = dim_map.get(dim.name.as_str()) {
                    dim.label = dc.label.clone();
                    if let Some(desc) = &dc.description {
                        dim.description = Some(desc.clone());
                    }
                }
            }
        }

        // Filter and relabel measures (own + induced) when listed explicitly
        if let Some(meas_cfgs) = &ec.measures {
            let meas_map: HashMap<&str, &WmFieldConfig> =
                meas_cfgs.iter().map(|m| (m.name.as_str(), m)).collect();
            entity
                .own_measures
                .retain(|m| meas_map.contains_key(m.name.as_str()));
            entity
                .induced_measures
                .retain(|m| meas_map.contains_key(m.name.as_str()));
            for m in entity.own_measures.iter_mut() {
                if let Some(mc) = meas_map.get(m.name.as_str()) {
                    m.label = mc.label.clone();
                    if let Some(desc) = &mc.description {
                        m.description = Some(desc.clone());
                    }
                }
            }
            for m in entity.induced_measures.iter_mut() {
                if let Some(mc) = meas_map.get(m.name.as_str()) {
                    m.label = mc.label.clone();
                    if let Some(desc) = &mc.description {
                        m.description = Some(desc.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod wm_config_tests {
    use super::super::world_model_config::{WmEntityConfig, WmFieldConfig, WorldModelConfig};
    use super::*;

    fn make_entity(id: &str) -> WmEntity {
        WmEntity {
            id: id.to_string(),
            label: id.to_string(),
            view: id.to_string(),
            description: None,
            depth: 0,
            display_field: None,
            dimensions: vec![
                WmDimension {
                    name: "dim_a".into(),
                    dim_type: "string".into(),
                    label: None,
                    description: None,
                },
                WmDimension {
                    name: "dim_b".into(),
                    dim_type: "number".into(),
                    label: None,
                    description: None,
                },
            ],
            own_measures: vec![WmMeasure {
                name: "revenue".into(),
                measure_type: MeasureType::Sum,
                additivity: AdditivityClass::Additive,
                label: None,
                description: None,
                expr: None,
                has_breakdown: false,
            }],
            induced_measures: vec![],
        }
    }

    fn s(v: &str) -> serde_json::Value {
        serde_json::Value::String(v.to_string())
    }

    fn in_values(f: &agentic_semantic::config::SemanticFilter) -> Vec<String> {
        match &f.filter_type {
            agentic_semantic::config::SemanticFilterType::In(a) => {
                let mut vs: Vec<String> = a
                    .values
                    .iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect();
                vs.sort();
                vs
            }
            _ => panic!("expected IN filter"),
        }
    }

    // A seed carrying only its first PK column (the instance-picker shape) must
    // not emit an empty `IN ()` for the child's remaining composite-FK columns —
    // that empty filter is what zeroed the Shipment/Return node counts.
    #[test]
    fn child_fk_filters_skips_columns_absent_from_parent() {
        let fk = vec![
            "order_shipments.order_id".to_string(),
            "order_shipments.line_item_id".to_string(),
        ];
        // Seed only constrains the first column (order_id = 2).
        let parent_pk_rows = vec![vec![s("2")]];
        let filters = child_fk_filters(&fk, &parent_pk_rows);
        assert_eq!(filters.len(), 1, "only the supplied column is filtered");
        assert_eq!(filters[0].field, "order_shipments.order_id");
        assert_eq!(in_values(&filters[0]), vec!["2".to_string()]);
    }

    // When the parent supplies all composite columns, every FK column is filtered
    // and duplicate values are de-duplicated.
    #[test]
    fn child_fk_filters_uses_all_columns_when_present() {
        let fk = vec![
            "order_shipments.order_id".to_string(),
            "order_shipments.line_item_id".to_string(),
        ];
        let parent_pk_rows = vec![vec![s("2"), s("4")], vec![s("2"), s("7")]];
        let filters = child_fk_filters(&fk, &parent_pk_rows);
        assert_eq!(filters.len(), 2);
        assert_eq!(in_values(&filters[0]), vec!["2".to_string()]);
        assert_eq!(
            in_values(&filters[1]),
            vec!["4".to_string(), "7".to_string()]
        );
    }

    // No usable values anywhere → no filters (caller skips the entity instead of
    // counting every row).
    #[test]
    fn child_fk_filters_empty_when_no_values() {
        let fk = vec!["v.fk".to_string()];
        let filters = child_fk_filters(&fk, &[]);
        assert!(filters.is_empty());
    }

    #[test]
    fn entity_not_in_config_is_filtered() {
        let mut entities = vec![make_entity("orders"), make_entity("customers")];
        let mut edges = vec![];
        let cfg = WorldModelConfig {
            entities: vec![WmEntityConfig {
                id: "orders".into(),
                label: None,
                description: None,
                display_field: None,
                dimensions: None,
                measures: None,
            }],
        };
        apply_world_model_config(&mut entities, &mut edges, &cfg);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].id, "orders");
    }

    #[test]
    fn label_and_display_field_applied() {
        let mut entities = vec![make_entity("orders")];
        let mut edges = vec![];
        let cfg = WorldModelConfig {
            entities: vec![WmEntityConfig {
                id: "orders".into(),
                label: Some("My Orders".into()),
                description: None,
                display_field: Some("customer_name".into()),
                dimensions: None,
                measures: None,
            }],
        };
        apply_world_model_config(&mut entities, &mut edges, &cfg);
        assert_eq!(entities[0].label, "My Orders");
        assert_eq!(entities[0].display_field.as_deref(), Some("customer_name"));
    }

    #[test]
    fn dimension_allowlist_filters_and_labels() {
        let mut entities = vec![make_entity("orders")];
        let mut edges = vec![];
        let cfg = WorldModelConfig {
            entities: vec![WmEntityConfig {
                id: "orders".into(),
                label: None,
                description: None,
                display_field: None,
                dimensions: Some(vec![WmFieldConfig {
                    name: "dim_b".into(),
                    label: Some("B Label".into()),
                    description: None,
                }]),
                measures: None,
            }],
        };
        apply_world_model_config(&mut entities, &mut edges, &cfg);
        assert_eq!(entities[0].dimensions.len(), 1);
        assert_eq!(entities[0].dimensions[0].name, "dim_b");
        assert_eq!(entities[0].dimensions[0].label.as_deref(), Some("B Label"));
    }

    #[test]
    fn absent_dimensions_shows_all() {
        let mut entities = vec![make_entity("orders")];
        let mut edges = vec![];
        let cfg = WorldModelConfig {
            entities: vec![WmEntityConfig {
                id: "orders".into(),
                label: None,
                description: None,
                display_field: None,
                dimensions: None,
                measures: None,
            }],
        };
        apply_world_model_config(&mut entities, &mut edges, &cfg);
        assert_eq!(entities[0].dimensions.len(), 2);
    }

    #[test]
    fn edges_filtered_when_endpoint_entity_hidden() {
        let mut entities = vec![make_entity("orders"), make_entity("customers")];
        let mut edges = vec![WmEdge {
            from: "orders".into(),
            to: "customers".into(),
            functional: true,
        }];
        let cfg = WorldModelConfig {
            entities: vec![WmEntityConfig {
                id: "orders".into(),
                label: None,
                description: None,
                display_field: None,
                dimensions: None,
                measures: None,
            }],
        };
        apply_world_model_config(&mut entities, &mut edges, &cfg);
        assert!(
            edges.is_empty(),
            "edge to hidden 'customers' must be removed"
        );
    }
}

/// `GET /{workspace_id}/semantic/world-model`
///
/// Returns the entity-centric world model: every primary entity in the
/// semantic layer, its own and induced measures (with operator and
/// additivity metadata), and the promotion edges between entities.
pub async fn get_world_model(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    layer_cache: SemanticLayerCacheCtx,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
) -> Result<extract::Json<WorldModelResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let semantics_path = workspace_manager.config_manager.semantics_scan_path();

    let layer = layer_cache.get_or_load(semantics_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to load semantic layer: {e}"),
            }),
        )
    })?;

    let promotions = Promotions::build(&layer.views).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to build promotion closure: {e}"),
            }),
        )
    })?;

    // Metric-tree component edges tell us which measures decompose: a measure
    // `view.name` has a breakdown when it is the target (`to`) of a component
    // edge. Built once and reused for every measure's `has_breakdown` flag.
    let breakdownable: std::collections::HashSet<String> = {
        use airlayer::engine::metric_tree::EdgeKind;
        let tree = oxy_semantic::build_metric_tree(&layer);
        tree.edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Component)
            .map(|e| e.to.clone())
            .collect()
    };

    let mut entities: Vec<WmEntity> = Vec::new();
    let mut edges: Vec<WmEdge> = Vec::new();

    for view in &layer.views {
        let Some(primary) = view
            .entities
            .iter()
            .find(|e| e.entity_type == EntityType::Primary)
        else {
            continue;
        };

        let entity_name = &primary.name;
        let depth = promotions.ancestry(entity_name).len();

        let dimensions: Vec<WmDimension> = view
            .dimensions
            .iter()
            .map(|d| WmDimension {
                name: d.name.clone(),
                dim_type: format!("{:?}", d.dimension_type).to_lowercase(),
                label: None,
                description: d.description.clone(),
            })
            .collect();

        let own_measures: Vec<WmMeasure> = view
            .measures
            .as_ref()
            .map(|ms| {
                ms.iter()
                    .filter(|m| !m.name.starts_with('_'))
                    .map(|m| WmMeasure {
                        name: m.name.clone(),
                        measure_type: m.measure_type.clone(),
                        additivity: m.measure_type.additivity_class(),
                        label: None,
                        description: m.description.clone(),
                        expr: m.expr.clone(),
                        has_breakdown: breakdownable.contains(&format!("{}.{}", view.name, m.name)),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let induced_measures: Vec<WmInducedMeasure> = promotions
            .induced_for_view(&view.name)
            .into_iter()
            .filter(|im| !im.source_measure.starts_with('_'))
            .map(|im| {
                let source = layer
                    .views
                    .iter()
                    .find(|v| v.name == im.source_view)
                    .and_then(|v| v.measures.as_ref())
                    .and_then(|ms| ms.iter().find(|m| m.name == im.source_measure));
                WmInducedMeasure {
                    name: im.source_measure.clone(),
                    measure_type: source
                        .map(|m| m.measure_type.clone())
                        .unwrap_or(MeasureType::Custom),
                    additivity: im.additivity,
                    label: None,
                    description: source.and_then(|m| m.description.clone()),
                    expr: source.and_then(|m| m.expr.clone()),
                    promoted_from: im.source_view.clone(),
                    path: im.path.clone(),
                }
            })
            .collect();

        if let Some(parent) = promotions.parent_of(entity_name) {
            edges.push(WmEdge {
                from: entity_name.clone(),
                to: parent.to_string(),
                functional: true,
            });
        }

        // FK cross-reference edges: Foreign entity declarations signal a join
        // relationship without a promotion hierarchy. Emit dashed edges so the
        // graph shows these structural cross-links alongside solid parent edges.
        for foreign in view
            .entities
            .iter()
            .filter(|e| e.entity_type == EntityType::Foreign)
        {
            let fk_target = &foreign.name;
            // Skip if already covered by the parent edge (avoids parallel edges).
            if promotions.parent_of(entity_name) == Some(fk_target.as_str()) {
                continue;
            }
            // Only draw an edge if the target is also a primary entity somewhere
            // in the layer (otherwise there's no node to connect to).
            let target_exists = layer.views.iter().any(|v| {
                v.entities
                    .iter()
                    .any(|e| e.entity_type == EntityType::Primary && e.name == *fk_target)
            });
            if target_exists {
                edges.push(WmEdge {
                    from: entity_name.clone(),
                    to: fk_target.clone(),
                    functional: false,
                });
            }
        }

        entities.push(WmEntity {
            id: entity_name.clone(),
            label: entity_name.clone(),
            view: view.name.clone(),
            description: view.description.clone(),
            depth,
            display_field: None,
            dimensions,
            own_measures,
            induced_measures,
        });
    }

    // Apply .world-model.yml display config if present (filter + label overrides).
    // Compile boundary first (serve replicas have no working copy), FS fallback.
    let workspace_path = workspace_manager.config_manager.workspace_path();
    match super::world_model_config::WorldModelConfig::resolve(
        layer_cache.workspace_id,
        workspace_path,
    )
    .await
    {
        Ok(Some(cfg)) => apply_world_model_config(&mut entities, &mut edges, &cfg),
        Ok(None) => {}
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse { message: e }),
            ));
        }
    }

    entities.sort_by_key(|e| e.depth);

    Ok(extract::Json(WorldModelResponse { entities, edges }))
}

/// `GET /{workspace_id}/semantic/world-model/instances`
pub async fn get_world_model_instances(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    axum::extract::State(app_state): axum::extract::State<crate::server::router::AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(q): axum::extract::Query<WmInstancesQuery>,
) -> Result<extract::Json<WmInstancesResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    // Cache the default (no-search) open: same entity, same workspace → same
    // result within the TTL. Search terms are not cached — they are diverse
    // across users and would fill the bounded cache with single-use entries.
    let is_search = q.search.as_deref().is_some_and(|s| !s.is_empty());
    let cache_key = if is_search {
        None
    } else {
        Some(format!("{}:{}:{}", workspace_id, q.entity, q.limit))
    };
    if let Some(ref key) = cache_key {
        if let Some(bytes) = app_state.query_result_cache.get(key) {
            if let Ok(cached) = serde_json::from_slice::<WmInstancesResponse>(&bytes) {
                return Ok(extract::Json(cached));
            }
        }
    }

    let semantics_path = workspace_manager.config_manager.semantics_scan_path();
    let layer = layer_cache.get_or_load(semantics_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: format!("Failed to load layer: {e}"),
            }),
        )
    })?;

    let view = primary_view_of(&layer, &q.entity).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Entity '{}' not found", q.entity),
            }),
        )
    })?;
    let table = view
        .table
        .as_deref()
        .or(view.sql.as_deref())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                extract::Json(ErrorResponse {
                    message: format!("Entity '{}' has no table", q.entity),
                }),
            )
        })?;
    let pk_cols = entity_keys_in_view(view, &q.entity, true);
    if pk_cols.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: format!("Entity '{}' has no key columns", q.entity),
            }),
        ));
    }

    // Look up display_field from .world-model.yml (silently ignore load errors here —
    // the instances endpoint is a picker convenience, not security-critical).
    // Compile boundary first (serve replicas have no working copy), FS fallback.
    let display_field = super::world_model_config::WorldModelConfig::resolve(
        workspace_id,
        workspace_manager.config_manager.workspace_path(),
    )
    .await
    .ok()
    .flatten()
    .and_then(|cfg| cfg.entities.into_iter().find(|e| e.id == q.entity))
    .and_then(|ec| ec.display_field);
    let disp = EntityDisplaySpec::for_entity(view, &q.entity, display_field.as_deref());

    // Build the semantic query: PK dimension(s) + optional display label dimension.
    // resolve_and_compile handles table aliasing, dialect, and database routing.
    let scan_path = workspace_manager.config_manager.semantics_scan_path();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    let order_by = disp.dims.first().cloned().unwrap_or_default();

    // Choose the search filter based on what the display field is:
    //   - text label dim  → Contains (substring LIKE)
    //   - PK as display   → Equals (exact key match)
    let search_filter: Vec<SemanticFilter> = match q.search.as_deref().filter(|s| !s.is_empty()) {
        None => vec![],
        Some(term) => {
            let (field, op) = if disp.has_label_dim {
                let label_field = disp.dims[disp.pk_count].clone();
                (
                    label_field,
                    SemanticFilterType::Contains(ScalarFilter { value: term.into() }),
                )
            } else {
                let pk_field = disp.dims.first().cloned().unwrap_or_default();
                (
                    pk_field,
                    SemanticFilterType::Eq(ScalarFilter { value: term.into() }),
                )
            };
            vec![SemanticFilter {
                field,
                filter_type: op,
            }]
        }
    };

    // For no-search: scan limit+1 to detect whether more records exist.
    let scan_limit = Some((q.limit as u64) + if is_search { 0 } else { 1 });

    let semantic_config = SemanticQueryConfig {
        topic: None,
        dimensions: disp.dims.clone(),
        measures: vec![],
        time_dimensions: vec![],
        filters: search_filter,
        orders: vec![SemanticOrder {
            field: order_by,
            direction: "asc".to_string(),
        }],
        limit: scan_limit,
        offset: None,
    };

    let layer_clone = (*layer).clone();
    let (base_sql, database_name) = tokio::task::spawn_blocking(move || {
        resolve_and_compile(
            &scan_path,
            &databases,
            &semantic_config,
            None,
            0,
            Some(layer_clone),
        )
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })
    .and_then(|compiled| match compiled {
        CompiledQuery::Warehouse { sql, database_name } => Ok((sql, database_name)),
        CompiledQuery::Preaggregation { preagg_sql, .. } => Ok((preagg_sql, String::new())),
    })?;

    let payload = SQLParams {
        sql: base_sql,
        database: database_name,
        filters: None,
        connections: None,
        result_format: None,
    };
    let rows = match run_via_agentic_connector(&workspace_manager, user.id, role, &payload).await {
        Ok(SemanticQueryResponse::Json(r)) => r,
        _ => vec![],
    };

    let all_items: Vec<WmInstanceItem> = rows
        .into_iter()
        .skip(1) // skip header row
        .map(|row| {
            let key = row.first().cloned().unwrap_or_default();
            let display = disp.display_from_row(&row);
            let display = if display.is_empty() {
                key.clone()
            } else {
                display
            };
            WmInstanceItem { key, display }
        })
        .collect();

    // For non-search we fetched limit+1 rows to detect overflow; trim to limit.
    let has_more = !is_search && all_items.len() > q.limit;
    let items: Vec<WmInstanceItem> = if has_more {
        all_items.into_iter().take(q.limit).collect()
    } else {
        all_items
    };
    let total = items.len();

    let response = WmInstancesResponse {
        total,
        has_more,
        items,
    };
    if let Some(key) = cache_key {
        if let Ok(bytes) = serde_json::to_vec(&response) {
            app_state.query_result_cache.insert(key, bytes);
        }
    }
    Ok(extract::Json(response))
}

/// `POST /{workspace_id}/semantic/world-model/filter-counts`
pub async fn post_world_model_filter_counts(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    engine_cache: SemanticEngineCacheCtx,
    axum::extract::State(app_state): axum::extract::State<crate::server::router::AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    extract::Json(req): extract::Json<WmFilterCountsRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, axum::Error>>>,
    (StatusCode, extract::Json<ErrorResponse>),
> {
    let semantics_path = workspace_manager.config_manager.semantics_scan_path();
    let layer = layer_cache.get_or_load(semantics_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;
    let promotions = Promotions::build(&layer.views).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;

    // Collect per-entity metadata needed to build semantic queries.
    struct EntityMeta {
        entity_name: String,
        view_name: String,
        datasource: String,
        // "{view}.{pk_dim}" refs — used in SemanticFilter fields and dimension selects.
        pk_dim_refs: Vec<String>,
        // Direct parent in the promotion hierarchy.
        parent_entity: Option<String>,
        // "{view}.{fk_dim}" refs pointing to the parent entity.
        fk_dim_refs: Vec<String>,
    }
    let entity_metas: Vec<EntityMeta> = layer
        .views
        .iter()
        .filter_map(|view| {
            let primary = view
                .entities
                .iter()
                .find(|e| e.entity_type == EntityType::Primary)?;
            let pk_names = entity_keys_in_view(view, &primary.name, true);
            if pk_names.is_empty() {
                return None;
            }
            let pk_dim_refs = pk_names
                .iter()
                .map(|k| format!("{}.{}", view.name, k))
                .collect();
            let parent_entity = promotions.parent_of(&primary.name).map(|s| s.to_string());
            let fk_dim_refs = if let Some(ref pe) = parent_entity {
                entity_keys_in_view(view, pe, false)
                    .into_iter()
                    .map(|k| format!("{}.{}", view.name, k))
                    .collect()
            } else {
                vec![]
            };
            Some(EntityMeta {
                entity_name: primary.name.clone(),
                view_name: view.name.clone(),
                datasource: view.datasource.clone().unwrap_or_default(),
                pk_dim_refs,
                parent_entity,
                fk_dim_refs,
            })
        })
        .collect();

    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    // Get-or-build the cached engine. The engine is `Send` but not `Sync`; it lives
    // behind a `Mutex` so each compile call locks, compiles, and drops the guard.
    let cached_engine = engine_cache
        .get_or_build(layer.clone(), databases.clone())
        .await;

    // Compile all total-count queries up front in one engine build, before the
    // spawn so we can move cached_engine/layer/databases into the spawned task.
    // The closure is wrapped in a block so it drops (releasing its borrows)
    // before we move those variables into the background spawn below.
    let total_sqls: Vec<Option<String>> = {
        let batch_compile_outer = |cfgs: Vec<SemanticQueryConfig>| {
            let engine_arc = cached_engine.clone();
            let layer_c = (*layer).clone();
            let dbs_c = databases.clone();
            tokio::task::spawn_blocking(move || {
                let compile_one = |cfg: &SemanticQueryConfig| -> Option<String> {
                    if let Some(ref arc) = engine_arc {
                        arc.lock()
                            .ok()
                            .and_then(|e| agentic_semantic::compile_with_engine(&e, cfg).ok())
                    } else {
                        let dialects =
                            airlayer::DatasourceDialectMap::from_config_databases(&dbs_c);
                        airlayer::SemanticEngine::from_semantic_layer(layer_c.clone(), dialects)
                            .ok()
                            .and_then(|e| agentic_semantic::compile_with_engine(&e, cfg).ok())
                    }
                };
                let sqls: Vec<Option<String>> = cfgs.iter().map(|cfg| compile_one(cfg)).collect();
                Ok::<_, agentic_semantic::SemanticError>(sqls)
            })
        };
        let total_cfgs: Vec<SemanticQueryConfig> = entity_metas
            .iter()
            .map(|meta| SemanticQueryConfig {
                topic: None,
                dimensions: vec![],
                measures: vec![format!("{}.__oxy_row_count", meta.view_name)],
                time_dimensions: vec![],
                filters: vec![],
                orders: vec![],
                limit: None,
                offset: None,
            })
            .collect();
        let t_compile = std::time::Instant::now();
        let sqls = batch_compile_outer(total_cfgs)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_else(|| vec![None; entity_metas.len()]);
        tracing::info!(
            elapsed_ms = t_compile.elapsed().as_millis(),
            "filter-counts: compiled total-count queries"
        );
        sqls
        // batch_compile_outer drops here — borrows of cached_engine/layer/databases end
    };

    // Extract only the fields needed by the total-count task so entity_metas
    // can be moved into the BFS task without cloning the full structs.
    struct TotalWork {
        entity_name: String,
        datasource: String,
        sql: String,
    }
    let total_works: Vec<TotalWork> = entity_metas
        .iter()
        .zip(total_sqls)
        .filter_map(|(meta, sql_opt)| {
            Some(TotalWork {
                entity_name: meta.entity_name.clone(),
                datasource: meta.datasource.clone(),
                sql: sql_opt?,
            })
        })
        .collect();

    let user_id = user.id;

    // Clone data needed by the background task before moving anything.
    let layer_inner = (*layer).clone();
    let wm_a = workspace_manager.clone();
    let wm_b = workspace_manager;
    let role_a = role.clone();
    let role_b = role;

    // ── Stream results back as they complete ──────────────────────────────
    //
    // Total counts (Task A) and BFS matched counts (Task B) run concurrently
    // inside a single spawned task.  Each result is sent through an mpsc
    // channel and yielded as an SSE event, so the browser sees node counts
    // appear progressively rather than waiting for all queries to finish.
    let (tx, rx) = tokio::sync::mpsc::channel::<WmFilterCountEvent>(256);

    tokio::spawn(async move {
        let tx_a = tx.clone();
        let tx_b = tx.clone();

        tokio::join!(
            // ── Task A: total count per entity — stream as each query completes
            async move {
                let mut futs: FuturesUnordered<_> = total_works
                    .into_iter()
                    .map(|w| {
                        let wm = wm_a.clone();
                        let role_c = role_a.clone();
                        async move {
                            let t0 = std::time::Instant::now();
                            let Ok(connector) =
                                build_connector(&wm, user_id, role_c, &w.datasource).await
                            else {
                                tracing::warn!(
                                    entity_name = %w.entity_name,
                                    "filter-counts: build_connector failed"
                                );
                                return (w.entity_name, 0u64);
                            };
                            let build_ms = t0.elapsed().as_millis();
                            let q0 = std::time::Instant::now();
                            let cnt = run_with_connector(&connector, &w.sql, &wm)
                                .await
                                .into_iter()
                                .next()
                                .and_then(|r| r.into_iter().next())
                                .and_then(|v: String| v.parse::<u64>().ok())
                                .unwrap_or(0);
                            let query_ms = q0.elapsed().as_millis();
                            tracing::debug!(
                                entity_name = %w.entity_name,
                                build_ms,
                                query_ms,
                                "filter-counts: total count"
                            );
                            (w.entity_name, cnt)
                        }
                    })
                    .collect();

                let t_exec = std::time::Instant::now();
                let mut n = 0usize;
                while let Some((name, total)) = futs.next().await {
                    n += 1;
                    tx_a.send(WmFilterCountEvent {
                        entity_name: name,
                        total: Some(total),
                        matched: None,
                        done: false,
                    })
                    .await
                    .ok();
                }
                tracing::info!(
                    elapsed_ms = t_exec.elapsed().as_millis(),
                    n,
                    "filter-counts: total counts streamed"
                );
            },
            // ── Task B: ancestor FK phase + BFS matched counts ────────────────
            //
            // BFS processes entities level by level (sorted by ancestry depth,
            // shallowest first) so that by the time we reach an entity we already
            // know the matching PK values of its parent, which become the In-filter
            // for this entity's FK.
            //
            // For ancestors of the seed (entities whose depth is LESS than the
            // seed's) we go the other direction: collect the FK values that the
            // bridge entity points to, and use those as an In-filter on this
            // ancestor's PK.
            // Task B: BFS matched counts — stream per depth level
            async move {
                // Reconstruct batch_compile inside this block so it's owned
                // (no reference to outer function stack after tokio::spawn).
                let batch_compile = |cfgs: Vec<SemanticQueryConfig>| {
                    let engine_arc = cached_engine.clone();
                    let layer_c = layer_inner.clone();
                    let dbs_c = databases.clone();
                    tokio::task::spawn_blocking(move || {
                        let compile_one = |cfg: &SemanticQueryConfig| -> Option<String> {
                            if let Some(ref arc) = engine_arc {
                                arc.lock().ok().and_then(|e| {
                                    agentic_semantic::compile_with_engine(&e, cfg).ok()
                                })
                            } else {
                                let dialects =
                                    airlayer::DatasourceDialectMap::from_config_databases(&dbs_c);
                                airlayer::SemanticEngine::from_semantic_layer(
                                    layer_c.clone(),
                                    dialects,
                                )
                                .ok()
                                .and_then(|e| agentic_semantic::compile_with_engine(&e, cfg).ok())
                            }
                        };
                        let sqls: Vec<Option<String>> =
                            cfgs.iter().map(|cfg| compile_one(cfg)).collect();
                        Ok::<_, agentic_semantic::SemanticError>(sqls)
                    })
                };

                // Seed matched = 1 (the record itself) — emit immediately.
                tx_b.send(WmFilterCountEvent {
                    entity_name: req.entity_id.clone(),
                    matched: Some(1),
                    total: None,
                    done: false,
                })
                .await
                .ok();

                // `matching_pks` maps entity_name → matched PK rows (each row is a
                // tuple of column values, one per pk_dim_ref column in order).
                // For single-PK entities this is Vec<Vec<1 value>>; for composite
                // keys (e.g. order_item with [order_id, line_item_id]) each inner
                // Vec holds all column values so per-column IN filters stay correct.
                let mut matching_pks: HashMap<String, Vec<Vec<serde_json::Value>>> = HashMap::new();
                matching_pks.insert(
                    req.entity_id.clone(),
                    vec![vec![serde_json::Value::String(req.key_value.clone())]],
                );

                let seed_ancestors = promotions.ancestry(&req.entity_id);
                let mut by_depth: std::collections::BTreeMap<usize, Vec<usize>> =
                    std::collections::BTreeMap::new();
                for (idx, meta) in entity_metas.iter().enumerate() {
                    if meta.entity_name == req.entity_id {
                        continue;
                    }
                    let depth = promotions.ancestry(&meta.entity_name).len();
                    by_depth.entry(depth).or_default().push(idx);
                }

                // Phase 1 (ancestors only): resolve FK values from bridge entities
                // before the main BFS loop so they don't block descendants.
                let mut ancestor_filters: HashMap<
                    String,
                    agentic_semantic::config::SemanticFilter,
                > = HashMap::new();

                let mut ancestor_idxs: Vec<usize> = entity_metas
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| {
                        m.entity_name != req.entity_id
                            && seed_ancestors.iter().any(|a| *a == m.entity_name)
                    })
                    .map(|(i, _)| i)
                    .collect();
                ancestor_idxs.sort_by_key(|&i| {
                    std::cmp::Reverse(promotions.ancestry(&entity_metas[i].entity_name).len())
                });

                struct AncestorWork {
                    entity_name: String,
                    pk_field: String,
                    fk_select_cfg: SemanticQueryConfig,
                    datasource: String,
                }
                let ancestor_works: Vec<AncestorWork> = ancestor_idxs
                    .iter()
                    .filter_map(|&idx| {
                        let meta = &entity_metas[idx];
                        let bridge =
                            child_of_toward(&promotions, &req.entity_id, &meta.entity_name)?;
                        let bridge_meta = entity_metas.iter().find(|m| m.entity_name == bridge)?;
                        let fk_ref = bridge_meta.fk_dim_refs.first()?.clone();
                        let pk_filter_field = bridge_meta.pk_dim_refs.first()?.clone();
                        let bridge_pk_rows = matching_pks.get(&bridge)?;
                        if bridge_pk_rows.is_empty() {
                            return None;
                        }
                        // Ancestor traversal uses the first PK column of the bridge
                        // entity (seeds and bridges are always single-column PKs here).
                        let bridge_pk_values: Vec<serde_json::Value> = bridge_pk_rows
                            .iter()
                            .filter_map(|r| r.first().cloned())
                            .collect();
                        if bridge_pk_values.is_empty() {
                            return None;
                        }
                        let pk_field = meta.pk_dim_refs.first()?.clone();
                        Some(AncestorWork {
                            entity_name: meta.entity_name.clone(),
                            pk_field,
                            fk_select_cfg: SemanticQueryConfig {
                                topic: None,
                                dimensions: vec![fk_ref],
                                measures: vec![],
                                time_dimensions: vec![],
                                filters: vec![agentic_semantic::config::SemanticFilter {
                                    field: pk_filter_field,
                                    filter_type: agentic_semantic::config::SemanticFilterType::In(
                                        agentic_semantic::config::ArrayFilter {
                                            values: bridge_pk_values,
                                        },
                                    ),
                                }],
                                orders: vec![],
                                limit: None,
                                offset: None,
                            },
                            datasource: bridge_meta.datasource.clone(),
                        })
                    })
                    .collect();

                if !ancestor_works.is_empty() {
                    let fk_cfgs: Vec<_> = ancestor_works
                        .iter()
                        .map(|w| w.fk_select_cfg.clone())
                        .collect();
                    let fk_sqls: Vec<Option<String>> = batch_compile(fk_cfgs)
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .unwrap_or_else(|| vec![None; ancestor_works.len()]);

                    let fk_futures: Vec<_> = ancestor_works
                        .into_iter()
                        .zip(fk_sqls)
                        .filter_map(|(work, sql_opt)| {
                            let sql = sql_opt?;
                            let wm = wm_b.clone();
                            let role_c = role_b.clone();
                            Some(async move {
                                let Ok(connector) =
                                    build_connector(&wm, user_id, role_c, &work.datasource).await
                                else {
                                    return (work.entity_name, work.pk_field, vec![]);
                                };
                                let vals = run_with_connector(&connector, &sql, &wm)
                                    .await
                                    .into_iter()
                                    .filter_map(|mut r: Vec<String>| {
                                        r.pop().map(serde_json::Value::String)
                                    })
                                    .collect::<Vec<_>>();
                                (work.entity_name, work.pk_field, vals)
                            })
                        })
                        .collect();

                    let fk_results = futures::future::join_all(fk_futures).await;
                    for (entity_name, pk_field, ancestor_pk_values) in fk_results {
                        if !ancestor_pk_values.is_empty() {
                            ancestor_filters.insert(
                                entity_name,
                                agentic_semantic::config::SemanticFilter {
                                    field: pk_field,
                                    filter_type: agentic_semantic::config::SemanticFilterType::In(
                                        agentic_semantic::config::ArrayFilter {
                                            values: ancestor_pk_values,
                                        },
                                    ),
                                },
                            );
                        }
                    }
                }

                // Phase 2: BFS level by level.
                let t_bfs = std::time::Instant::now();
                for (depth, idxs) in &by_depth {
                    struct LevelWork<'a> {
                        meta: &'a EntityMeta,
                        count_cfg: SemanticQueryConfig,
                        pk_cfg: Option<SemanticQueryConfig>,
                    }

                    let level_works: Vec<LevelWork<'_>> = idxs
                        .iter()
                        .filter_map(|&idx| {
                            let meta = &entity_metas[idx];

                            let entity_ancestors = promotions.ancestry(&meta.entity_name);
                            // Build one SemanticFilter per FK column so composite
                            // keys (e.g. shipment → order_item via order_id +
                            // line_item_id) are filtered on every column the parent
                            // actually constrains.  See `child_fk_filters` for why
                            // columns absent from the parent's PK rows are skipped
                            // rather than turned into an empty `IN ()`.
                            let filters: Vec<agentic_semantic::config::SemanticFilter> =
                                if entity_ancestors
                                    .iter()
                                    .any(|a| *a == req.entity_id.as_str())
                                {
                                    let parent = meta.parent_entity.as_deref()?;
                                    let parent_pk_rows = matching_pks.get(parent)?;
                                    if parent_pk_rows.is_empty() {
                                        return None;
                                    }
                                    let f = child_fk_filters(&meta.fk_dim_refs, parent_pk_rows);
                                    // No usable column values → can't constrain this
                                    // child; skip rather than count every row.
                                    if f.is_empty() {
                                        return None;
                                    }
                                    f
                                } else if seed_ancestors
                                    .iter()
                                    .any(|a| *a == meta.entity_name.as_str())
                                {
                                    vec![ancestor_filters.get(&meta.entity_name)?.clone()]
                                } else {
                                    return None;
                                };

                            let has_children =
                                !promotions.children_of(&meta.entity_name).is_empty();
                            let count_cfg = SemanticQueryConfig {
                                topic: None,
                                dimensions: vec![],
                                measures: vec![format!("{}.__oxy_row_count", meta.view_name)],
                                time_dimensions: vec![],
                                filters: filters.clone(),
                                orders: vec![],
                                limit: None,
                                offset: None,
                            };
                            let pk_cfg = has_children.then(|| SemanticQueryConfig {
                                topic: None,
                                dimensions: meta.pk_dim_refs.clone(),
                                measures: vec![],
                                time_dimensions: vec![],
                                filters,
                                orders: vec![],
                                limit: None,
                                offset: None,
                            });
                            Some(LevelWork {
                                meta,
                                count_cfg,
                                pk_cfg,
                            })
                        })
                        .collect();

                    if level_works.is_empty() {
                        continue;
                    }

                    let all_cfgs: Vec<SemanticQueryConfig> = level_works
                        .iter()
                        .flat_map(|w| {
                            let mut v = vec![w.count_cfg.clone()];
                            if let Some(ref pk) = w.pk_cfg {
                                v.push(pk.clone());
                            }
                            v
                        })
                        .collect();

                    let t_level_compile = std::time::Instant::now();
                    let all_sqls: Vec<Option<String>> = batch_compile(all_cfgs)
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .unwrap_or_else(|| {
                            vec![
                                None;
                                level_works
                                    .iter()
                                    .map(|w| if w.pk_cfg.is_some() { 2 } else { 1 })
                                    .sum()
                            ]
                        });
                    tracing::info!(
                        depth,
                        elapsed_ms = t_level_compile.elapsed().as_millis(),
                        "filter-counts BFS: compiled level queries"
                    );

                    let t_level_exec = std::time::Instant::now();
                    let mut sql_iter = all_sqls.into_iter();
                    struct EntityResult {
                        entity_name: String,
                        matched: u64,
                        pks: Option<Vec<Vec<serde_json::Value>>>,
                    }
                    let exec_futures: Vec<_> = level_works
                        .iter()
                        .map(|w| {
                            let count_sql_opt = sql_iter.next().flatten();
                            let pk_sql_opt =
                                w.pk_cfg.as_ref().and_then(|_| sql_iter.next().flatten());
                            let datasource = w.meta.datasource.clone();
                            let entity_name = w.meta.entity_name.clone();
                            let wm = wm_b.clone();
                            let role_c = role_b.clone();
                            let depth_val = *depth;
                            async move {
                                let t0 = std::time::Instant::now();
                                let connector = build_connector(&wm, user_id, role_c, &datasource)
                                    .await
                                    .ok();
                                let build_ms = t0.elapsed().as_millis();
                                let q0 = std::time::Instant::now();
                                let matched = match (count_sql_opt, connector.as_ref()) {
                                    (Some(sql), Some(c)) => run_with_connector(c, &sql, &wm)
                                        .await
                                        .into_iter()
                                        .next()
                                        .and_then(|r| r.into_iter().next())
                                        .and_then(|v: String| v.parse::<u64>().ok())
                                        .unwrap_or(0),
                                    _ => 0,
                                };
                                let count_ms = q0.elapsed().as_millis();
                                let pk0 = std::time::Instant::now();
                                let pks = match (pk_sql_opt, connector.as_ref()) {
                                    (Some(sql), Some(c)) if matched > 0 => Some(
                                        run_with_connector(c, &sql, &wm)
                                            .await
                                            .into_iter()
                                            .map(|r: Vec<String>| {
                                                r.into_iter()
                                                    .map(serde_json::Value::String)
                                                    .collect()
                                            })
                                            .collect::<Vec<_>>(),
                                    ),
                                    (Some(_), _) => Some(vec![]),
                                    _ => None,
                                };
                                let pk_ms = pk0.elapsed().as_millis();
                                tracing::debug!(
                                    %entity_name,
                                    depth = depth_val,
                                    build_ms,
                                    count_ms,
                                    pk_ms,
                                    "filter-counts BFS: entity timings"
                                );
                                EntityResult {
                                    entity_name,
                                    matched,
                                    pks,
                                }
                            }
                        })
                        .collect();

                    let results = futures::future::join_all(exec_futures).await;
                    tracing::info!(
                        depth,
                        elapsed_ms = t_level_exec.elapsed().as_millis(),
                        n = results.len(),
                        "filter-counts BFS: executed level queries"
                    );
                    for r in results {
                        tx_b.send(WmFilterCountEvent {
                            entity_name: r.entity_name.clone(),
                            matched: Some(r.matched),
                            total: None,
                            done: false,
                        })
                        .await
                        .ok();
                        if let Some(pks) = r.pks {
                            matching_pks.insert(r.entity_name, pks);
                        }
                    }
                }
                tracing::info!(
                    elapsed_ms = t_bfs.elapsed().as_millis(),
                    "filter-counts BFS: all levels done"
                );
            }
        );

        // Both tasks finished — send done sentinel then drop tx (closes the channel).
        tx.send(WmFilterCountEvent {
            entity_name: String::new(),
            total: None,
            matched: None,
            done: true,
        })
        .await
        .ok();
    });

    Ok(Sse::new(create_sse_stream(rx)).keep_alive(KeepAlive::default()))
}

// ── World Model: instance measure breakdown (driver tree) ───────────────────

#[derive(Debug, Deserialize)]
pub struct WmMeasureBreakdownQuery {
    pub entity: String,
    pub key: String,
    pub measure: String,
    #[serde(default)]
    pub datasource: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WmBreakdownNode {
    /// Metric node id `view.measure`.
    pub id: String,
    pub view: String,
    pub measure: String,
    pub label: String,
    pub measure_type: String,
    pub is_composite: bool,
    pub is_root: bool,
    pub expr: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WmBreakdownEdge {
    pub from: String,
    pub to: String,
    /// "add" | "sub" | "mul" | "div".
    pub operator: String,
    pub sign: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WmMeasureBreakdownEvent {
    Init {
        root: String,
        nodes: Vec<WmBreakdownNode>,
        edges: Vec<WmBreakdownEdge>,
    },
    Value {
        node_id: String,
        value: Option<String>,
        unvalued_reason: Option<String>,
    },
    Done,
}

/// Map an airlayer subtree (component edges only) into UI node/edge DTOs.
/// Returns None when `root_id` is absent from the tree.
fn breakdown_structure(
    tree: &airlayer::engine::metric_tree::MetricTree,
    root_id: &str,
) -> Option<(Vec<WmBreakdownNode>, Vec<WmBreakdownEdge>)> {
    use airlayer::engine::metric_tree::{EdgeKind, EdgeOperator};
    let sub = tree.subtree(root_id)?;
    let nodes = sub
        .nodes
        .iter()
        .map(|n| WmBreakdownNode {
            id: n.id.clone(),
            view: n.view.clone(),
            measure: n.measure.clone(),
            label: n.label.clone(),
            measure_type: n.measure_type.clone(),
            is_composite: n.is_composite,
            is_root: n.id == root_id,
            expr: n.expr.clone(),
        })
        .collect();
    let edges = sub
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Component)
        .map(|e| WmBreakdownEdge {
            from: e.from.clone(),
            to: e.to.clone(),
            operator: match e.operator {
                EdgeOperator::Add => "add",
                EdgeOperator::Sub => "sub",
                EdgeOperator::Mul => "mul",
                EdgeOperator::Div => "div",
            }
            .to_string(),
            sign: e.sign,
        })
        .collect();
    Some((nodes, edges))
}

/// PK-equality filters for an instance key. `key_values.len()==1` → first PK col;
/// otherwise zip each PK col with its value (composite key).
fn build_pk_filters(
    view_name: &str,
    pk_cols: &[String],
    key_values: &[String],
) -> Vec<agentic_semantic::config::SemanticFilter> {
    use agentic_semantic::config::{ScalarFilter, SemanticFilter, SemanticFilterType};
    let eq = |field: String, val: &str| SemanticFilter {
        field,
        filter_type: SemanticFilterType::Eq(ScalarFilter {
            value: serde_json::Value::String(val.to_string()),
        }),
    };
    if key_values.len() == 1 {
        vec![eq(
            format!(
                "{view_name}.{}",
                pk_cols.first().cloned().unwrap_or_default()
            ),
            &key_values[0],
        )]
    } else {
        pk_cols
            .iter()
            .zip(key_values)
            .map(|(c, v)| eq(format!("{view_name}.{c}"), v))
            .collect()
    }
}

/// Filters that scope `target_view` to the instance.
/// - `target_view == primary_view` → PK filters.
/// - else → FK column for `entity` in `target_view` eq the first key value.
///
/// Returns None when no FK path resolves (the node will be streamed unvalued).
fn instance_filter_for_view(
    target_view: &airlayer::View,
    entity: &str,
    key_values: &[String],
    pk_cols: &[String],
    primary_view: &str,
) -> Option<Vec<agentic_semantic::config::SemanticFilter>> {
    if target_view.name == primary_view {
        return Some(build_pk_filters(&target_view.name, pk_cols, key_values));
    }
    let fk = entity_keys_in_view(target_view, entity, false);
    let fk_col = fk.first()?;
    Some(build_pk_filters(
        &target_view.name,
        std::slice::from_ref(fk_col),
        &key_values[..1],
    ))
}

/// In-memory valuation plan for a breakdown: one `SemanticQueryConfig` per view
/// group (measures = that view's subtree nodes, in node order), plus the node ids
/// that have no join path to the instance (streamed unvalued).
struct BreakdownValuePlan {
    /// (view_name, node_ids in column order, config).
    groups: Vec<(String, Vec<String>, SemanticQueryConfig)>,
    unvalued: Vec<String>,
}

fn breakdown_value_plan(
    layer: &airlayer::SemanticLayer,
    nodes: &[WmBreakdownNode],
    entity: &str,
    key_values: &[String],
    pk_cols: &[String],
    primary_view: &str,
) -> BreakdownValuePlan {
    use std::collections::BTreeMap;
    // Preserve node order within each view group so columns map back to node ids.
    let mut by_view: BTreeMap<String, Vec<&WmBreakdownNode>> = BTreeMap::new();
    for n in nodes {
        by_view.entry(n.view.clone()).or_default().push(n);
    }
    let mut groups = Vec::new();
    let mut unvalued = Vec::new();
    for (view_name, group_nodes) in by_view {
        let Some(target_view) = layer.views.iter().find(|v| v.name == view_name) else {
            unvalued.extend(group_nodes.iter().map(|n| n.id.clone()));
            continue;
        };
        let Some(filters) =
            instance_filter_for_view(target_view, entity, key_values, pk_cols, primary_view)
        else {
            unvalued.extend(group_nodes.iter().map(|n| n.id.clone()));
            continue;
        };
        let cfg = SemanticQueryConfig {
            topic: None,
            dimensions: vec![],
            measures: group_nodes
                .iter()
                .map(|n| format!("{}.{}", n.view, n.measure))
                .collect(),
            time_dimensions: vec![],
            filters,
            orders: vec![],
            limit: Some(1),
            offset: None,
        };
        groups.push((
            view_name,
            group_nodes.iter().map(|n| n.id.clone()).collect(),
            cfg,
        ));
    }
    BreakdownValuePlan { groups, unvalued }
}

/// `GET /{workspace_id}/semantic/world-model/instance-detail`
///
/// Streams `WmInstanceDetailEvent` via SSE so the panel renders progressively:
/// `init` (attributes) appears first, then `parent`, then individual `child` events,
/// then `measures`, then `done`.
pub async fn get_world_model_instance_detail(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    axum::extract::Query(q): axum::extract::Query<WmInstanceDetailQuery>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, axum::Error>>>,
    (StatusCode, extract::Json<ErrorResponse>),
> {
    let semantics_path = workspace_manager.config_manager.semantics_scan_path();
    let layer = layer_cache.get_or_load(semantics_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;
    let promotions = Promotions::build(&layer.views).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse {
                message: e.to_string(),
            }),
        )
    })?;

    let view = primary_view_of(&layer, &q.entity).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Entity '{}' not found", q.entity),
            }),
        )
    })?;
    let pk_cols = entity_keys_in_view(view, &q.entity, true);
    let datasource = view.datasource.clone().unwrap_or_default();

    // Load .world-model.yml config once — used for display_field across primary, child,
    // and parent entities. Silently ignore load errors (display degrades to PK fallback).
    // Compile boundary first (serve replicas have no working copy), FS fallback.
    let wm_cfg = super::world_model_config::WorldModelConfig::resolve(
        layer_cache.workspace_id,
        workspace_manager.config_manager.workspace_path(),
    )
    .await
    .ok()
    .flatten();
    // Per-entity allowlist + labels for the PRIMARY entity, used to filter and relabel
    // the attribute and measure sections (mirrors apply_world_model_config for the graph).
    // `None` means no allowlist → show everything observed in the view (current behavior).
    let primary_entity_cfg = wm_cfg
        .as_ref()
        .and_then(|cfg| cfg.entities.iter().find(|e| e.id == q.entity));
    // Ordered (name, label) list for dimensions; preserves config ordering in the panel.
    let dim_allow: Option<Vec<(String, Option<String>)>> = primary_entity_cfg
        .and_then(|ec| ec.dimensions.as_ref())
        .map(|dims| {
            dims.iter()
                .map(|d| (d.name.clone(), d.label.clone()))
                .collect()
        });
    // name -> label map for measures (own + induced).
    let meas_allow: Option<std::collections::HashMap<String, Option<String>>> = primary_entity_cfg
        .and_then(|ec| ec.measures.as_ref())
        .map(|ms| {
            ms.iter()
                .map(|m| (m.name.clone(), m.label.clone()))
                .collect()
        });

    let get_display_field = move |entity_id: &str| -> Option<String> {
        wm_cfg
            .as_ref()
            .and_then(|cfg| cfg.entities.iter().find(|e| e.id == entity_id))
            .and_then(|ec| ec.display_field.clone())
    };

    // Build the primary connector once for this datasource.
    let connector = build_connector(&workspace_manager, user.id, role.clone(), &datasource)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                extract::Json(ErrorResponse {
                    message: e.to_string(),
                }),
            )
        })?;

    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();

    // Shared PK filters used across all per-instance queries.
    //
    // `q.key` is either:
    //   • a plain string — the first PK column value, as returned by the instance picker
    //     (`WmInstanceItem::key = row.first()`).  We can only filter on that one column.
    //   • a JSON array of per-column values — what `WmChildSample::sample_keys` encodes
    //     for composite-PK children so all columns are available.
    //
    // Only expand to N filters when we have N values; a single value always maps to the
    // first PK column so the picker flow is unchanged.
    let key_values: Vec<String> =
        serde_json::from_str::<Vec<String>>(&q.key).unwrap_or_else(|_| vec![q.key.clone()]);
    let pk_filters: Vec<agentic_semantic::config::SemanticFilter> = if key_values.len() == 1 {
        vec![agentic_semantic::config::SemanticFilter {
            field: format!(
                "{}.{}",
                view.name,
                pk_cols.first().cloned().unwrap_or_default()
            ),
            filter_type: agentic_semantic::config::SemanticFilterType::Eq(
                agentic_semantic::config::ScalarFilter {
                    value: serde_json::Value::String(key_values[0].clone()),
                },
            ),
        }]
    } else {
        pk_cols
            .iter()
            .zip(key_values.iter())
            .map(|(col, val)| agentic_semantic::config::SemanticFilter {
                field: format!("{}.{}", view.name, col),
                filter_type: agentic_semantic::config::SemanticFilterType::Eq(
                    agentic_semantic::config::ScalarFilter {
                        value: serde_json::Value::String(val.clone()),
                    },
                ),
            })
            .collect()
    };

    // --- Collect all query configs upfront (pure in-memory, no blocking) ---
    let dim_names: Vec<String> = view.dimensions.iter().map(|d| d.name.clone()).collect();
    let display_field = get_display_field(&q.entity);
    let entity_display = EntityDisplaySpec::for_entity(view, &q.entity, display_field.as_deref());

    // 1. Attrs config
    let attrs_cfg = SemanticQueryConfig {
        topic: None,
        dimensions: dim_names
            .iter()
            .map(|n| format!("{}.{}", view.name, n))
            .collect(),
        measures: vec![],
        time_dimensions: vec![],
        filters: pk_filters.clone(),
        orders: vec![],
        limit: Some(1),
        offset: None,
    };

    // 3. Child configs — (label, sample_cfg, count_cfg)
    struct ChildCfg {
        label: String,
        sample: SemanticQueryConfig,
        count: SemanticQueryConfig,
        // pk_count: how many leading columns are PK values.
        // has_label_dim: whether a label column follows the PK columns.
        // Display = label col when present, else join pk cols with " · ".
        pk_count: usize,
        has_label_dim: bool,
    }
    let child_cfgs: Vec<ChildCfg> = promotions
        .children_of(&q.entity)
        .iter()
        .filter_map(|child_entity| {
            let child_view = primary_view_of(&layer, child_entity)?;
            let child_pk = entity_keys_in_view(child_view, child_entity, true);
            let fk_in_child = entity_keys_in_view(child_view, &q.entity, false);
            if child_pk.is_empty() || fk_in_child.is_empty() {
                return None;
            }
            let fk_filter = agentic_semantic::config::SemanticFilter {
                field: format!("{}.{}", child_view.name, fk_in_child[0]),
                filter_type: agentic_semantic::config::SemanticFilterType::Eq(
                    agentic_semantic::config::ScalarFilter {
                        value: serde_json::Value::String(q.key.clone()),
                    },
                ),
            };
            let child_display_field = get_display_field(child_entity);
            let child_disp = EntityDisplaySpec::for_entity(
                child_view,
                child_entity,
                child_display_field.as_deref(),
            );
            let pk_count = child_disp.pk_count;
            let has_label_dim = child_disp.has_label_dim;
            Some(ChildCfg {
                label: format!("{child_entity} → {}", q.entity),
                sample: SemanticQueryConfig {
                    topic: None,
                    dimensions: child_disp.dims,
                    measures: vec![],
                    time_dimensions: vec![],
                    filters: vec![fk_filter.clone()],
                    orders: vec![],
                    limit: Some(5),
                    offset: None,
                },
                count: SemanticQueryConfig {
                    topic: None,
                    dimensions: vec![],
                    measures: vec![count_measure_ref(child_view)],
                    time_dimensions: vec![],
                    filters: vec![fk_filter],
                    orders: vec![],
                    limit: Some(1),
                    offset: None,
                },
                pk_count,
                has_label_dim,
            })
        })
        .collect();

    // 4. Own measures — one batch query with ALL measures → 1 row, M columns.
    struct MeasureMeta {
        name: String,
        measure_type: String,
        label: Option<String>,
    }
    let own_measures: Vec<_> = view
        .measures
        .as_ref()
        .map(|ms| {
            ms.iter()
                .filter(|m| !m.name.starts_with('_'))
                .filter(|m| {
                    meas_allow
                        .as_ref()
                        .is_none_or(|a| a.contains_key(m.name.as_str()))
                })
                .collect()
        })
        .unwrap_or_default();
    let measure_meta: Vec<MeasureMeta> = own_measures
        .iter()
        .map(|m| MeasureMeta {
            name: m.name.clone(),
            measure_type: format!("{:?}", m.measure_type).to_lowercase(),
            label: meas_allow
                .as_ref()
                .and_then(|a| a.get(m.name.as_str()).cloned().flatten()),
        })
        .collect();
    let own_batch_cfg = SemanticQueryConfig {
        topic: None,
        dimensions: vec![],
        measures: own_measures
            .iter()
            .map(|m| format!("{}.{}", view.name, m.name))
            .collect(),
        time_dimensions: vec![],
        filters: pk_filters.clone(),
        orders: vec![],
        limit: Some(1),
        offset: None,
    };

    // Induced measures — group by source_view so each source gets ONE value
    // query (all its measures as columns) + ONE count query.
    struct InducedGroup {
        source_view_name: String,
        // (name, measure_type, label) — count is the last query column, not listed here.
        measures: Vec<(String, String, Option<String>)>,
        cfg: SemanticQueryConfig,
    }
    let mut induced_by_source: std::collections::HashMap<
        String,
        Vec<(String, String, Option<String>)>,
    > = std::collections::HashMap::new();
    for im in promotions
        .induced_for_view(&view.name)
        .into_iter()
        .filter(|im| !im.source_measure.starts_with('_'))
        .filter(|im| {
            meas_allow
                .as_ref()
                .is_none_or(|a| a.contains_key(im.source_measure.as_str()))
        })
    {
        if let Some(source_view) = layer.views.iter().find(|v| v.name == im.source_view) {
            if let Some(sm) = source_view
                .measures
                .as_ref()
                .and_then(|ms| ms.iter().find(|m| m.name == im.source_measure))
            {
                let label = meas_allow
                    .as_ref()
                    .and_then(|a| a.get(im.source_measure.as_str()).cloned().flatten());
                induced_by_source
                    .entry(im.source_view.clone())
                    .or_default()
                    .push((
                        im.source_measure.clone(),
                        format!("{:?}", sm.measure_type).to_lowercase(),
                        label,
                    ));
            }
        }
    }
    let induced_groups: Vec<InducedGroup> = induced_by_source
        .into_iter()
        .filter_map(|(source_view_name, measures)| {
            let sv = layer.views.iter().find(|v| v.name == source_view_name)?;
            let mut all_measure_refs: Vec<String> = measures
                .iter()
                .map(|(n, _, _)| format!("{}.{}", source_view_name, n))
                .collect();
            all_measure_refs.push(count_measure_ref(sv));
            Some(InducedGroup {
                cfg: SemanticQueryConfig {
                    topic: None,
                    dimensions: vec![],
                    measures: all_measure_refs,
                    time_dimensions: vec![],
                    filters: pk_filters.clone(),
                    orders: vec![],
                    limit: Some(1),
                    offset: None,
                },
                source_view_name,
                measures,
            })
        })
        .collect();

    let (child_sample_cfgs, child_count_cfgs): (Vec<_>, Vec<_>) = child_cfgs
        .iter()
        .map(|cc| (cc.sample.clone(), cc.count.clone()))
        .unzip();
    let induced_cfgs: Vec<SemanticQueryConfig> =
        induced_groups.iter().map(|g| g.cfg.clone()).collect();

    // --- Phase 1: compile ALL SQL configs (except parent which needs FK from attrs) ---
    let layer_clone = (*layer).clone();
    let dbs_clone = databases.clone();
    type SqlOpt = Option<String>;
    let phase1: Option<(SqlOpt, Vec<SqlOpt>, Vec<SqlOpt>, SqlOpt, Vec<SqlOpt>)> =
        tokio::task::spawn_blocking(move || {
            let dialects = airlayer::DatasourceDialectMap::from_config_databases(&dbs_clone);
            let engine = airlayer::SemanticEngine::from_semantic_layer(layer_clone, dialects)
                .map_err(|e| agentic_semantic::SemanticError::Runtime(e.to_string()))?;
            let c = |cfg: &SemanticQueryConfig| {
                let result = agentic_semantic::compile_with_engine(&engine, cfg);
                if let Err(ref e) = result {
                    tracing::warn!(error = %e, "instance-detail SQL compilation failed");
                }
                result.ok()
            };
            Ok::<_, agentic_semantic::SemanticError>((
                c(&attrs_cfg),
                child_sample_cfgs.iter().map(|cc| c(cc)).collect(),
                child_count_cfgs.iter().map(|cc| c(cc)).collect(),
                c(&own_batch_cfg),
                induced_cfgs.iter().map(|ic| c(ic)).collect(),
            ))
        })
        .await
        .ok()
        .and_then(|r| r.ok());
    let (attrs_sql, child_sample_sqls, child_count_sqls, own_batch_sql, induced_sqls) =
        phase1.unwrap_or_default();

    // --- Stream results: three concurrent tasks via tokio::join! ---
    //
    // Task A: attrs → emit Init → Phase 2 parent compile → exec parent → emit Parent
    // Task B: FuturesUnordered over children (each: join!(sample, count)) → emit Child
    // Task C: join_all(own_batch + induced) → emit Measures
    let (tx, rx) = tokio::sync::mpsc::channel::<WmInstanceDetailEvent>(128);
    let tx_a = tx.clone();
    let tx_b = tx.clone();
    let tx_c = tx.clone();

    let connector_a = connector.clone();
    let connector_b = connector.clone();
    let connector_c = connector;
    let wm_a = workspace_manager.clone();
    let wm_b = workspace_manager.clone();
    let wm_c = workspace_manager;

    tokio::spawn(async move {
        tokio::join!(
            // ── Task A: attrs row → Init event → Phase 2 parent → Parent event ──
            async move {
                let attr_rows = match attrs_sql {
                    Some(ref sql) => run_with_connector(&connector_a, sql, &wm_a).await,
                    None => vec![],
                };
                let attr_row = attr_rows.into_iter().next().unwrap_or_default();

                let attr_values: Vec<(String, String)> = dim_names
                    .iter()
                    .enumerate()
                    .map(|(i, name)| (name.clone(), attr_row.get(i).cloned().unwrap_or_default()))
                    .collect();

                let display = {
                    let d = entity_display.display_from_attrs(&attr_values);
                    if d.is_empty() { q.key.clone() } else { d }
                };

                // Filter + relabel attributes per the .world-model.yml allowlist when present;
                // otherwise emit every observed dimension. The query above always selects all
                // dimensions so parent-FK resolution below still works regardless of the filter.
                let attributes: Vec<WmAttrValue> = match &dim_allow {
                    Some(allow) => {
                        let value_map: std::collections::HashMap<&str, &str> = attr_values
                            .iter()
                            .map(|(n, v)| (n.as_str(), v.as_str()))
                            .collect();
                        allow
                            .iter()
                            .filter_map(|(name, label)| {
                                value_map.get(name.as_str()).map(|v| WmAttrValue {
                                    name: name.clone(),
                                    value: v.to_string(),
                                    label: label.clone(),
                                })
                            })
                            .collect()
                    }
                    None => attr_values
                        .into_iter()
                        .map(|(name, value)| WmAttrValue {
                            name,
                            value,
                            label: None,
                        })
                        .collect(),
                };

                tx_a.send(WmInstanceDetailEvent::Init {
                    entity_id: q.entity.clone(),
                    key_value: q.key.clone(),
                    display,
                    attributes,
                })
                .await
                .ok();

                // Phase 2: compile + exec parent lookup (FK value now known from attr_row).
                let mut promotes_to: Vec<WmParentRef> = vec![];
                'parent: {
                    let Some(parent_entity) = promotions.parent_of(&q.entity) else {
                        break 'parent;
                    };
                    let Some(parent_view) = primary_view_of(&layer, parent_entity) else {
                        break 'parent;
                    };
                    let parent_pk = entity_keys_in_view(parent_view, parent_entity, true);
                    let Some(this_view) = primary_view_of(&layer, &q.entity) else {
                        break 'parent;
                    };
                    let fk_cols = entity_keys_in_view(this_view, parent_entity, false);
                    let fk_value = fk_cols
                        .first()
                        .and_then(|fk| {
                            dim_names
                                .iter()
                                .position(|n| n == fk)
                                .and_then(|i| attr_row.get(i).cloned())
                        })
                        .unwrap_or_else(|| q.key.clone());
                    let parent_display_field = get_display_field(parent_entity);
                    let parent_disp = EntityDisplaySpec::for_entity(
                        parent_view,
                        parent_entity,
                        parent_display_field.as_deref(),
                    );
                    let parent_cfg = SemanticQueryConfig {
                        topic: None,
                        dimensions: parent_disp.dims.clone(),
                        measures: vec![],
                        time_dimensions: vec![],
                        filters: vec![agentic_semantic::config::SemanticFilter {
                            field: format!(
                                "{}.{}",
                                parent_view.name,
                                parent_pk.first().cloned().unwrap_or_default()
                            ),
                            filter_type: agentic_semantic::config::SemanticFilterType::Eq(
                                agentic_semantic::config::ScalarFilter {
                                    value: serde_json::Value::String(fk_value.clone()),
                                },
                            ),
                        }],
                        orders: vec![],
                        limit: Some(1),
                        offset: None,
                    };
                    let layer_clone2 = (*layer).clone();
                    let dbs_clone2 = databases.clone();
                    let parent_sql = tokio::task::spawn_blocking(move || {
                        let dialects =
                            airlayer::DatasourceDialectMap::from_config_databases(&dbs_clone2);
                        let engine = airlayer::SemanticEngine::from_semantic_layer(
                            layer_clone2,
                            dialects,
                        )
                        .map_err(|e| agentic_semantic::SemanticError::Runtime(e.to_string()))?;
                        agentic_semantic::compile_with_engine(&engine, &parent_cfg)
                    })
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                    let parent_rows = match parent_sql {
                        Some(ref sql) => run_with_connector(&connector_a, sql, &wm_a).await,
                        None => vec![],
                    };
                    let parent_display = parent_rows
                        .into_iter()
                        .next()
                        .map(|row| {
                            let d = parent_disp.display_from_row(&row);
                            if d.is_empty() { fk_value.clone() } else { d }
                        })
                        .unwrap_or_else(|| fk_value.clone());
                    promotes_to.push(WmParentRef {
                        promotion: format!("{} → {parent_entity}", q.entity),
                        key: fk_value,
                        display: parent_display,
                    });
                }
                tx_a.send(WmInstanceDetailEvent::Parent { promotes_to })
                    .await
                    .ok();
            },
            // ── Task B: children — FuturesUnordered, each child: join!(sample, count) ──
            async move {
                let mut futs: FuturesUnordered<_> = child_cfgs
                    .into_iter()
                    .zip(child_sample_sqls.into_iter().zip(child_count_sqls))
                    .map(|(cc, (sample_sql, count_sql))| {
                        let c = connector_b.clone();
                        let wm = wm_b.clone();
                        async move {
                            let c2 = c.clone();
                            let wm2 = wm.clone();
                            let (sample_rows, count_rows) = tokio::join!(
                                async move {
                                    match sample_sql {
                                        Some(ref sql) => run_with_connector(&c, sql, &wm).await,
                                        None => vec![],
                                    }
                                },
                                async move {
                                    match count_sql {
                                        Some(ref sql) => run_with_connector(&c2, sql, &wm2).await,
                                        None => vec![],
                                    }
                                },
                            );
                            let fiber_count = count_rows
                                .into_iter()
                                .next()
                                .and_then(|r| r.into_iter().next())
                                .and_then(|v| v.parse::<u64>().ok())
                                .unwrap_or(0);
                            let (sample, sample_keys): (Vec<String>, Vec<String>) = sample_rows
                                .into_iter()
                                .map(|r| {
                                    let pk_vals = &r[..cc.pk_count.min(r.len())];
                                    // navigation key: plain string for single PK,
                                    // JSON array for composite so it round-trips cleanly.
                                    let nav_key = if cc.pk_count <= 1 {
                                        r.first().cloned().unwrap_or_default()
                                    } else {
                                        serde_json::to_string(
                                            &pk_vals.iter().cloned().collect::<Vec<_>>(),
                                        )
                                        .unwrap_or_else(|_| r.first().cloned().unwrap_or_default())
                                    };
                                    let display = if cc.has_label_dim {
                                        r.get(cc.pk_count).cloned().unwrap_or_else(|| {
                                            r.first().cloned().unwrap_or_default()
                                        })
                                    } else {
                                        let parts: Vec<&str> = pk_vals
                                            .iter()
                                            .map(|s| s.as_str())
                                            .filter(|s| !s.is_empty())
                                            .collect();
                                        if parts.is_empty() {
                                            r.first().cloned().unwrap_or_default()
                                        } else {
                                            parts.join(" · ")
                                        }
                                    };
                                    (display, nav_key)
                                })
                                .unzip();
                            WmChildSample {
                                promotion: cc.label,
                                fiber_count,
                                sample,
                                sample_keys,
                            }
                        }
                    })
                    .collect();

                while let Some(child) = futs.next().await {
                    tx_b.send(WmInstanceDetailEvent::Child { child }).await.ok();
                }
            },
            // ── Task C: own measures + induced — FuturesUnordered for streaming ──
            // Emits MeasureNames immediately (schema-only, no DB), then one Measure event
            // per completed query group so the frontend can fill skeletons progressively.
            async move {
                // Phase C-0: emit all measure names/types derived from schema — no DB needed.
                let mut measure_names: Vec<WmMeasureName> = measure_meta
                    .iter()
                    .map(|m| WmMeasureName {
                        name: m.name.clone(),
                        measure_type: m.measure_type.clone(),
                        label: m.label.clone(),
                    })
                    .collect();
                for group in &induced_groups {
                    for (name, measure_type, label) in &group.measures {
                        measure_names.push(WmMeasureName {
                            name: name.clone(),
                            measure_type: measure_type.clone(),
                            label: label.clone(),
                        });
                    }
                }
                tx_c.send(WmInstanceDetailEvent::MeasureNames { measure_names })
                    .await
                    .ok();

                // Phase C-1: run all query groups concurrently; emit each as it finishes.
                // Tag: None = own batch, Some(idx) = induced_groups[idx].
                type Rows = Vec<Vec<String>>;
                let mut futs: FuturesUnordered<
                    std::pin::Pin<
                        Box<dyn std::future::Future<Output = (Option<usize>, Rows)> + Send>,
                    >,
                > = FuturesUnordered::new();

                {
                    let c = connector_c.clone();
                    let wm = wm_c.clone();
                    futs.push(Box::pin(async move {
                        let rows = match own_batch_sql {
                            Some(ref sql) => run_with_connector(&c, sql, &wm).await,
                            None => vec![],
                        };
                        (None, rows)
                    }));
                }
                for (idx, sql_opt) in induced_sqls.into_iter().enumerate() {
                    let c = connector_c.clone();
                    let wm = wm_c.clone();
                    futs.push(Box::pin(async move {
                        let rows = match sql_opt {
                            Some(ref sql) => run_with_connector(&c, sql, &wm).await,
                            None => vec![],
                        };
                        (Some(idx), rows)
                    }));
                }

                while let Some((tag, rows)) = futs.next().await {
                    let computed_measures: Vec<WmComputedMeasure> = match tag {
                        None => {
                            let own_row = rows.into_iter().next().unwrap_or_default();
                            measure_meta
                                .iter()
                                .enumerate()
                                .map(|(i, meta)| WmComputedMeasure {
                                    name: meta.name.clone(),
                                    measure_type: meta.measure_type.clone(),
                                    value: own_row
                                        .get(i)
                                        .cloned()
                                        .unwrap_or_else(|| "—".to_string()),
                                    fiber_count: 1,
                                    label: meta.label.clone(),
                                })
                                .collect()
                        }
                        Some(idx) => {
                            let group = &induced_groups[idx];
                            let row = rows.into_iter().next().unwrap_or_default();
                            let fiber_count =
                                row.last().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                            group
                                .measures
                                .iter()
                                .enumerate()
                                .map(|(i, (name, measure_type, label))| WmComputedMeasure {
                                    name: name.clone(),
                                    measure_type: measure_type.clone(),
                                    value: row.get(i).cloned().unwrap_or_else(|| "—".to_string()),
                                    fiber_count,
                                    label: label.clone(),
                                })
                                .collect()
                        }
                    };
                    tx_c.send(WmInstanceDetailEvent::Measure { computed_measures })
                        .await
                        .ok();
                }
            },
        );

        tx.send(WmInstanceDetailEvent::Done).await.ok();
    });

    Ok(Sse::new(create_sse_stream(rx)).keep_alive(KeepAlive::default()))
}

/// `GET /{workspace_id}/semantic/world-model/measure-breakdown`
///
/// Streams the metric-tree subtree for `measure` at `entity`, valued at the
/// instance `key`: `init` (structure) → per-node `value` events → `done`.
pub async fn get_world_model_measure_breakdown(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
    axum::extract::Query(q): axum::extract::Query<WmMeasureBreakdownQuery>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, axum::Error>>>,
    (StatusCode, extract::Json<ErrorResponse>),
> {
    let err500 = |e: String| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            extract::Json(ErrorResponse { message: e }),
        )
    };
    let semantics_path = workspace_manager.config_manager.semantics_scan_path();
    let layer = layer_cache
        .get_or_load(semantics_path)
        .await
        .map_err(|e| err500(e.to_string()))?;

    let view = primary_view_of(&layer, &q.entity).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Entity '{}' not found", q.entity),
            }),
        )
    })?;
    let primary_view = view.name.clone();
    let pk_cols = entity_keys_in_view(view, &q.entity, true);
    let datasource = q
        .datasource
        .clone()
        .or_else(|| view.datasource.clone())
        .unwrap_or_default();
    let root_id = format!("{}.{}", primary_view, q.measure);

    let tree = oxy_semantic::build_metric_tree(&layer);
    let (nodes, edges) = breakdown_structure(&tree, &root_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            extract::Json(ErrorResponse {
                message: format!("Measure '{root_id}' not found in metric tree"),
            }),
        )
    })?;

    let key_values: Vec<String> =
        serde_json::from_str::<Vec<String>>(&q.key).unwrap_or_else(|_| vec![q.key.clone()]);

    let plan = breakdown_value_plan(
        &layer,
        &nodes,
        &q.entity,
        &key_values,
        &pk_cols,
        &primary_view,
    );

    // Compile all view-group SQLs up front (pure, blocking).
    let layer_clone = (*layer).clone();
    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();
    let cfgs: Vec<SemanticQueryConfig> = plan.groups.iter().map(|(_, _, c)| c.clone()).collect();
    let compiled: Vec<Option<String>> = tokio::task::spawn_blocking(move || {
        let dialects = airlayer::DatasourceDialectMap::from_config_databases(&databases);
        let engine = match airlayer::SemanticEngine::from_semantic_layer(layer_clone, dialects) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "measure-breakdown: engine build failed");
                return vec![None; cfgs.len()];
            }
        };
        cfgs.iter()
            .map(|cfg| {
                agentic_semantic::compile_with_engine(&engine, cfg)
                    .map_err(|e| {
                        tracing::warn!(error = %e, "measure-breakdown: SQL compile failed");
                    })
                    .ok()
            })
            .collect()
    })
    .await
    .unwrap_or_else(|_| vec![None; plan.groups.len()]);

    let connector = build_connector(&workspace_manager, user.id, role, &datasource)
        .await
        .map_err(|e| err500(e.to_string()))?;

    let (tx, rx) = tokio::sync::mpsc::channel::<WmMeasureBreakdownEvent>(64);
    let group_node_ids: Vec<Vec<String>> =
        plan.groups.iter().map(|(_, ids, _)| ids.clone()).collect();
    let unvalued = plan.unvalued.clone();

    tokio::spawn(async move {
        tx.send(WmMeasureBreakdownEvent::Init {
            root: root_id,
            nodes,
            edges,
        })
        .await
        .ok();

        for node_id in unvalued {
            tx.send(WmMeasureBreakdownEvent::Value {
                node_id,
                value: None,
                unvalued_reason: Some("no join path to instance".to_string()),
            })
            .await
            .ok();
        }

        // Run each view-group query concurrently; emit one Value per node.
        let mut futs: FuturesUnordered<_> = compiled
            .into_iter()
            .zip(group_node_ids)
            .map(|(sql, node_ids)| {
                let connector = connector.clone();
                let wm = workspace_manager.clone();
                async move {
                    let rows = match sql {
                        Some(ref s) => run_with_connector(&connector, s, &wm).await,
                        None => vec![],
                    };
                    (node_ids, rows.into_iter().next().unwrap_or_default())
                }
            })
            .collect();

        while let Some((node_ids, row)) = futs.next().await {
            for (i, node_id) in node_ids.into_iter().enumerate() {
                let value = row.get(i).cloned();
                tx.send(WmMeasureBreakdownEvent::Value {
                    node_id,
                    value,
                    unvalued_reason: None,
                })
                .await
                .ok();
            }
        }

        tx.send(WmMeasureBreakdownEvent::Done).await.ok();
    });

    Ok(Sse::new(create_sse_stream(rx)).keep_alive(KeepAlive::default()))
}

// ── POST /semantic/compile ─────────────────────────────────────────────────

#[cfg(test)]
mod breakdown_tests {
    use super::*;
    use airlayer::engine::metric_tree::{
        EdgeKind, EdgeOperator, MetricEdge, MetricNode, MetricTree,
    };
    use airlayer::schema::models::{DriverConfidence, DriverDirection, DriverForm, DriverStrength};

    fn node(id: &str, view: &str, measure: &str, composite: bool) -> MetricNode {
        MetricNode {
            id: id.into(),
            view: view.into(),
            measure: measure.into(),
            label: measure.into(),
            description: None,
            measure_type: "number".into(),
            is_composite: composite,
            expr: None,
        }
    }

    // Revenue(composite) = Orders(*) × Aov(*), all on the `store` view.
    fn sample_tree() -> MetricTree {
        let mul = |from: &str, to: &str| MetricEdge {
            from: from.into(),
            to: to.into(),
            kind: EdgeKind::Component,
            sign: 1.0,
            operator: EdgeOperator::Mul,
            direction: DriverDirection::default(),
            strength: DriverStrength::Strong,
            confidence: DriverConfidence::High,
            coefficient: None,
            form: DriverForm::default(),
            intercept: None,
            lag: None,
            description: None,
            refs: None,
        };
        MetricTree {
            nodes: vec![
                node("store.revenue", "store", "revenue", true),
                node("store.orders", "store", "orders", false),
                node("store.aov", "store", "aov", false),
            ],
            edges: vec![
                mul("store.orders", "store.revenue"),
                mul("store.aov", "store.revenue"),
            ],
            root: None,
        }
    }

    #[test]
    fn structure_includes_root_and_component_children() {
        let tree = sample_tree();
        let (nodes, edges) = breakdown_structure(&tree, "store.revenue").unwrap();
        assert_eq!(nodes.len(), 3);
        assert!(nodes.iter().any(|n| n.id == "store.revenue" && n.is_root));
        assert!(!edges.is_empty());
        assert!(edges.iter().all(|e| e.operator == "mul"));
    }

    #[test]
    fn leaf_measure_yields_single_node() {
        let tree = sample_tree();
        let (nodes, edges) = breakdown_structure(&tree, "store.orders").unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(edges.is_empty());
    }

    #[test]
    fn primary_view_uses_pk_filter() {
        let cols = vec!["store_id".to_string()];
        let f = build_pk_filters("store", &cols, &["s1".to_string()]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].field, "store.store_id");
    }
}
