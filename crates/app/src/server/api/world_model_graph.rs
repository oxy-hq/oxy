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
    ArrayFilter, ScalarFilter, SemanticFilter, SemanticFilterType, SemanticOrder,
    SemanticQueryConfig,
};
use entity::workspace_members::WorkspaceRole;
use oxy::adapters::workspace::manager::WorkspaceManager;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use uuid::Uuid;

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
    /// Sample of reachable descendant rows at this entity's grain (display strings).
    /// Empty for ancestors, the seed, and total-only events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample: Vec<String>,
    /// Navigation keys aligned with `sample` (see `sample_row_to_display_key`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_keys: Vec<String>,
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

/// One navigable relationship an entity participates in, used by the instance
/// drill-down traversal. Direction is implicit: the entity that owns this link
/// is the *finer* (child / fan-out) side and references `target_entity`'s PK via
/// `fk_dim_refs`, so `target_entity` is the coarser side.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityLink {
    /// The entity on the other end of the relationship.
    target_entity: String,
    /// "{view}.{fk_col}" refs on the child (self) side pointing at `target`'s PK.
    fk_dim_refs: Vec<String>,
    /// Whether this is the solid `parent:` spine or a dashed foreign cross-link.
    kind: LinkKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkKind {
    /// The `parent:` declaration — the single solid hierarchy edge.
    Parent,
    /// A Foreign entity that resolves to a Primary elsewhere — a dashed edge.
    CrossLink,
}

/// Build the navigable link set for the Primary entity of `view`.
///
/// This is the union of the two relationship sources that already feed the
/// *drawn* edges (`world_model_graph` edge builder): the `parent:` spine and
/// every Foreign entity whose name resolves to a Primary entity elsewhere in
/// the layer. The parent is itself always declared as a Foreign entity (it must
/// be, to carry the FK column), so iterating Foreign declarations captures both;
/// the parent one is tagged `LinkKind::Parent`, the rest `CrossLink`.
///
/// The resulting navigable graph is exactly the drawn graph — no edge appears in
/// the traversal that the user can't already see.
fn build_entity_links(
    views: &[airlayer::View],
    view: &airlayer::View,
    parent_entity: Option<&str>,
) -> Vec<EntityLink> {
    view.entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Foreign)
        .filter_map(|foreign| {
            // Only navigable if the target is a Primary entity somewhere.
            let target_exists = views.iter().any(|v| {
                v.entities
                    .iter()
                    .any(|e| e.entity_type == EntityType::Primary && e.name == foreign.name)
            });
            if !target_exists {
                return None;
            }
            let fk_dim_refs: Vec<String> = foreign
                .get_keys()
                .into_iter()
                .map(|k| format!("{}.{}", view.name, k))
                .collect();
            if fk_dim_refs.is_empty() {
                return None;
            }
            let kind = if parent_entity == Some(foreign.name.as_str()) {
                LinkKind::Parent
            } else {
                LinkKind::CrossLink
            };
            Some(EntityLink {
                target_entity: foreign.name.clone(),
                fk_dim_refs,
                kind,
            })
        })
        .collect()
}

/// Per-entity metadata needed to build the instance drill-down semantic
/// queries. Hoisted out of the filter-counts handler so the expansion-plan
/// helpers below can be unit-tested against it.
struct EntityMeta {
    entity_name: String,
    view_name: String,
    datasource: String,
    /// "{view}.{pk_dim}" refs — used in SemanticFilter fields and dimension selects.
    pk_dim_refs: Vec<String>,
    /// Navigable relationships this entity participates in (parent spine +
    /// foreign cross-links). Drives the undirected instance-drill-down BFS.
    links: Vec<EntityLink>,
    /// Dims to SELECT for a sample row: PK dims first, then label dim if any.
    sample_dims: Vec<String>,
    pk_count: usize,
    has_label_dim: bool,
}

/// Reverse adjacency over the navigable link graph: target entity → its
/// *finer* neighbours, i.e. (child index, the child's FK refs pointing at
/// this target). This is the inbound direction the `parent:` spine alone
/// never provides for cross-links (e.g. `store ← order`). Built once and
/// shared by every traversal (schema reachability, the direct-join fast
/// path, and the cross-datasource legacy BFS fallback) instead of being
/// recomputed inline at each call site.
fn build_inbound_index(entity_metas: &[EntityMeta]) -> HashMap<&str, Vec<(usize, &[String])>> {
    let mut inbound: HashMap<&str, Vec<(usize, &[String])>> = HashMap::new();
    for (i, meta) in entity_metas.iter().enumerate() {
        for link in &meta.links {
            inbound
                .entry(link.target_entity.as_str())
                .or_default()
                .push((i, link.fk_dim_refs.as_slice()));
        }
    }
    inbound
}

/// Every entity index reachable from `seed_idx` via the undirected link graph
/// (parent spine + FK cross-links, both directions) — the same edges drawn in
/// the graph UI. This is a static property of the `.view.yml` relationships
/// alone, so unlike matched-row reachability it needs no IO and can be
/// computed once up front instead of discovered hop-by-hop at query time.
fn schema_reachable_entities(
    entity_metas: &[EntityMeta],
    inbound: &HashMap<&str, Vec<(usize, &[String])>>,
    meta_idx: &HashMap<&str, usize>,
    seed_idx: usize,
) -> Vec<usize> {
    let mut visited: std::collections::HashSet<usize> = std::collections::HashSet::from([seed_idx]);
    let mut stack = vec![seed_idx];
    let mut result = Vec::new();
    while let Some(idx) = stack.pop() {
        for link in &entity_metas[idx].links {
            if let Some(&t) = meta_idx.get(link.target_entity.as_str())
                && visited.insert(t)
            {
                result.push(t);
                stack.push(t);
            }
        }
        if let Some(children) = inbound.get(entity_metas[idx].entity_name.as_str()) {
            for &(child_idx, _) in children {
                if visited.insert(child_idx) {
                    result.push(child_idx);
                    stack.push(child_idx);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod schema_reachability_tests {
    use super::*;

    fn meta(name: &str, datasource: &str, links: Vec<EntityLink>) -> EntityMeta {
        EntityMeta {
            entity_name: name.to_string(),
            view_name: name.to_string(),
            datasource: datasource.to_string(),
            pk_dim_refs: vec![format!("{name}.id")],
            links,
            sample_dims: vec![format!("{name}.id")],
            pk_count: 1,
            has_label_dim: false,
        }
    }

    fn link(target: &str, kind: LinkKind) -> EntityLink {
        EntityLink {
            target_entity: target.to_string(),
            fk_dim_refs: vec![format!("self.{target}_id")],
            kind,
        }
    }

    fn idx(metas: &[EntityMeta], name: &str) -> usize {
        metas.iter().position(|m| m.entity_name == name).unwrap()
    }

    // seed -> parent (outbound), child -> seed (inbound cross-link): both
    // neighbours must be reachable regardless of edge direction.
    #[test]
    fn reaches_both_outbound_and_inbound_neighbours() {
        let metas = vec![
            meta("seed", "db1", vec![link("parent", LinkKind::Parent)]),
            meta("parent", "db1", vec![]),
            meta("child", "db1", vec![link("seed", LinkKind::CrossLink)]),
        ];
        let meta_idx: HashMap<&str, usize> = metas
            .iter()
            .enumerate()
            .map(|(i, m)| (m.entity_name.as_str(), i))
            .collect();
        let inbound = build_inbound_index(&metas);
        let seed_idx = idx(&metas, "seed");
        let mut reachable = schema_reachable_entities(&metas, &inbound, &meta_idx, seed_idx);
        reachable.sort();
        let mut expected = vec![idx(&metas, "parent"), idx(&metas, "child")];
        expected.sort();
        assert_eq!(reachable, expected);
    }

    // A chain seed -> a -> b -> c must all be reachable, however deep —
    // schema reachability doesn't stop at direct neighbours.
    #[test]
    fn reaches_transitively_through_a_chain() {
        let metas = vec![
            meta("seed", "db1", vec![link("a", LinkKind::Parent)]),
            meta("a", "db1", vec![link("b", LinkKind::Parent)]),
            meta("b", "db1", vec![link("c", LinkKind::Parent)]),
            meta("c", "db1", vec![]),
        ];
        let meta_idx: HashMap<&str, usize> = metas
            .iter()
            .enumerate()
            .map(|(i, m)| (m.entity_name.as_str(), i))
            .collect();
        let inbound = build_inbound_index(&metas);
        let seed_idx = idx(&metas, "seed");
        let mut reachable = schema_reachable_entities(&metas, &inbound, &meta_idx, seed_idx);
        reachable.sort();
        let mut expected = vec![idx(&metas, "a"), idx(&metas, "b"), idx(&metas, "c")];
        expected.sort();
        assert_eq!(reachable, expected);
    }

    // An entity with no path to the seed at all is not reachable.
    #[test]
    fn excludes_disconnected_entities() {
        let metas = vec![
            meta("seed", "db1", vec![link("a", LinkKind::Parent)]),
            meta("a", "db1", vec![]),
            meta("island", "db1", vec![]),
        ];
        let meta_idx: HashMap<&str, usize> = metas
            .iter()
            .enumerate()
            .map(|(i, m)| (m.entity_name.as_str(), i))
            .collect();
        let inbound = build_inbound_index(&metas);
        let seed_idx = idx(&metas, "seed");
        let reachable = schema_reachable_entities(&metas, &inbound, &meta_idx, seed_idx);
        assert_eq!(reachable, vec![idx(&metas, "a")]);
    }

    // The seed's own index is never included in its reachable set.
    #[test]
    fn excludes_seed_itself() {
        let metas = vec![
            meta("seed", "db1", vec![link("a", LinkKind::Parent)]),
            meta("a", "db1", vec![]),
        ];
        let meta_idx: HashMap<&str, usize> = metas
            .iter()
            .enumerate()
            .map(|(i, m)| (m.entity_name.as_str(), i))
            .collect();
        let inbound = build_inbound_index(&metas);
        let seed_idx = idx(&metas, "seed");
        let reachable = schema_reachable_entities(&metas, &inbound, &meta_idx, seed_idx);
        assert!(!reachable.contains(&seed_idx));
    }
}

/// Column layout of an entity's single "expansion" query, which selects its PK
/// columns, each outbound link's FK column, and its label column **in one shot**
/// (all functionally determined by the PK, so grouping by them doesn't change
/// row cardinality). One query then yields everything a BFS hop needs: the
/// matched count, the PK rows for inbound children, the FK values for outbound
/// targets, and the display sample — replacing the old separate count + pk +
/// fk-select + sample queries.
struct WmExpansionPlan {
    /// Dimensions to SELECT, in column order.
    dims: Vec<String>,
    /// Column index of each PK dimension (in `pk_dim_refs` order).
    pk_cols: Vec<usize>,
    /// Column index of the label dimension, if the entity has one.
    label_col: Option<usize>,
    /// (target entity, column index of that link's first FK column).
    link_cols: Vec<(String, usize)>,
}

/// Build the expansion column layout for `meta`. De-duplicates dimensions that
/// coincide (e.g. a PK column reused as an FK) via a first-seen index map.
fn wm_expansion_plan(meta: &EntityMeta) -> WmExpansionPlan {
    let mut dims: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    fn col_of(r: &str, dims: &mut Vec<String>, index: &mut HashMap<String, usize>) -> usize {
        if let Some(&i) = index.get(r) {
            return i;
        }
        let i = dims.len();
        dims.push(r.to_string());
        index.insert(r.to_string(), i);
        i
    }
    let pk_cols: Vec<usize> = meta
        .pk_dim_refs
        .iter()
        .map(|r| col_of(r, &mut dims, &mut index))
        .collect();
    let link_cols: Vec<(String, usize)> = meta
        .links
        .iter()
        .filter_map(|l| {
            l.fk_dim_refs
                .first()
                .map(|r| (l.target_entity.clone(), col_of(r, &mut dims, &mut index)))
        })
        .collect();
    // The label dim (if any) is the last entry of `sample_dims` (PK dims first).
    let label_col = if meta.has_label_dim {
        meta.sample_dims
            .last()
            .map(|r| col_of(r, &mut dims, &mut index))
    } else {
        None
    };
    WmExpansionPlan {
        dims,
        pk_cols,
        label_col,
        link_cols,
    }
}

/// Result of executing one entity's expansion query.
struct WmExpansionResult {
    /// Distinct-PK count — the node's `matched` value.
    matched: u64,
    /// Distinct PK rows (all columns), for building inbound-child FK filters.
    pk_rows: Vec<Vec<serde_json::Value>>,
    /// Per outbound link: (target entity, that link's distinct FK values).
    fk_values: Vec<(String, Vec<serde_json::Value>)>,
    /// Up to 3 display strings and their nav keys.
    sample: Vec<String>,
    sample_keys: Vec<String>,
}

/// Parse the rows of an expansion query into a [`WmExpansionResult`] using the
/// column layout from [`wm_expansion_plan`]. Pure — no IO — so it is unit-tested
/// directly. `matched` counts **distinct** PK tuples (an FK fanning out to
/// several rows per instance never inflates the count).
fn parse_expansion_rows(rows: &[Vec<String>], plan: &WmExpansionPlan) -> WmExpansionResult {
    let project = |row: &[String], cols: &[usize]| -> Vec<String> {
        cols.iter()
            .map(|&c| row.get(c).cloned().unwrap_or_default())
            .collect()
    };

    let mut seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
    let mut pk_rows: Vec<Vec<serde_json::Value>> = Vec::new();
    for row in rows {
        let key = project(row, &plan.pk_cols);
        if seen.insert(key.clone()) {
            pk_rows.push(key.into_iter().map(serde_json::Value::String).collect());
        }
    }
    let matched = pk_rows.len() as u64;

    let fk_values: Vec<(String, Vec<serde_json::Value>)> = plan
        .link_cols
        .iter()
        .map(|(target, col)| {
            let vals: Vec<serde_json::Value> = rows
                .iter()
                .filter_map(|r| r.get(*col).cloned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .map(serde_json::Value::String)
                .collect();
            (target.clone(), vals)
        })
        .collect();

    let pk_count = plan.pk_cols.len();
    let has_label = plan.label_col.is_some();
    // Dedup by PK tuple before taking the first 3 so a single instance fanning
    // out to several FK rows never wastes a preview slot (mirrors `seen`/`pk_rows`).
    let mut sample_seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();
    let (sample, sample_keys): (Vec<String>, Vec<String>) = rows
        .iter()
        .filter(|&row| sample_seen.insert(project(row, &plan.pk_cols)))
        .take(3)
        .map(|row| {
            let mut proj = project(row, &plan.pk_cols);
            if let Some(lc) = plan.label_col {
                proj.push(row.get(lc).cloned().unwrap_or_default());
            }
            sample_row_to_display_key(&proj, pk_count, has_label)
        })
        .unzip();

    WmExpansionResult {
        matched,
        pk_rows,
        fk_values,
        sample,
        sample_keys,
    }
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
        if let Some(ref lbl) = self.label_name
            && let Some((_, v)) = attrs.iter().find(|(n, _)| n == lbl)
            && !v.is_empty()
        {
            return v.clone();
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

/// Turn a sample SELECT row into `(display, nav_key)`.
///
/// Columns are ordered `[pk_0, .., pk_{pk_count-1}, (label?)]` per
/// `EntityDisplaySpec::dims`. `nav_key` is the canonical key the instance
/// endpoints accept: the plain first PK value for single-PK entities, or a
/// JSON array string of the PK columns for composite PKs. `display` prefers the
/// label column, falling back to PK columns joined with " · ".
fn sample_row_to_display_key(
    row: &[String],
    pk_count: usize,
    has_label_dim: bool,
) -> (String, String) {
    let pk_vals = &row[..pk_count.min(row.len())];
    let nav_key = if pk_count <= 1 {
        row.first().cloned().unwrap_or_default()
    } else {
        serde_json::to_string(&pk_vals.to_vec())
            .unwrap_or_else(|_| row.first().cloned().unwrap_or_default())
    };
    let display = if has_label_dim {
        row.get(pk_count)
            .cloned()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| join_pk_parts(pk_vals, row))
    } else {
        join_pk_parts(pk_vals, row)
    };
    (display, nav_key)
}

/// Join non-empty PK column values with " · "; fall back to the first column.
fn join_pk_parts(pk_vals: &[String], row: &[String]) -> String {
    let parts: Vec<&str> = pk_vals
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

    fn wm_view(name: &str, entities: serde_json::Value) -> airlayer::View {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "table": name,
            "entities": entities,
            "dimensions": [],
        }))
        .expect("valid view")
    }

    /// The `examples/` star-schema slice: `order` rolls up to `customer` (its
    /// declared parent) and also foreign-references `retail_store` and
    /// `shipping_address`. The link set must union the parent spine with the
    /// foreign cross-links, tagging each correctly.
    #[test]
    fn build_entity_links_unions_parent_and_cross_links() {
        let orders = wm_view(
            "orders",
            serde_json::json!([
                {"name": "order", "type": "primary", "key": "order_id", "parent": "customer"},
                {"name": "customer", "type": "foreign", "key": "customer_id"},
                {"name": "shipping_address", "type": "foreign", "key": "shipping_address_id"},
                {"name": "retail_store", "type": "foreign", "key": "store_id"},
            ]),
        );
        let customers = wm_view(
            "customers",
            serde_json::json!([{"name": "customer", "type": "primary", "key": "customer_id"}]),
        );
        let stores = wm_view(
            "stores",
            serde_json::json!([{"name": "retail_store", "type": "primary", "key": "store_id"}]),
        );
        let shipping = wm_view(
            "shipping_addresses",
            serde_json::json!([
                {"name": "shipping_address", "type": "primary", "key": "shipping_address_id"},
            ]),
        );
        let views = vec![orders.clone(), customers, stores, shipping];

        let links = build_entity_links(&views, &orders, Some("customer"));
        assert_eq!(
            links,
            vec![
                EntityLink {
                    target_entity: "customer".into(),
                    fk_dim_refs: vec!["orders.customer_id".into()],
                    kind: LinkKind::Parent,
                },
                EntityLink {
                    target_entity: "shipping_address".into(),
                    fk_dim_refs: vec!["orders.shipping_address_id".into()],
                    kind: LinkKind::CrossLink,
                },
                EntityLink {
                    target_entity: "retail_store".into(),
                    fk_dim_refs: vec!["orders.store_id".into()],
                    kind: LinkKind::CrossLink,
                },
            ],
        );
    }

    fn link(target: &str, fk: &str, kind: LinkKind) -> EntityLink {
        EntityLink {
            target_entity: target.into(),
            fk_dim_refs: vec![fk.into()],
            kind,
        }
    }

    fn meta_for(
        name: &str,
        view: &str,
        pk_dim_refs: Vec<String>,
        links: Vec<EntityLink>,
        sample_dims: Vec<String>,
        has_label_dim: bool,
    ) -> EntityMeta {
        EntityMeta {
            entity_name: name.into(),
            view_name: view.into(),
            datasource: "local".into(),
            pk_count: pk_dim_refs.len(),
            pk_dim_refs,
            links,
            sample_dims,
            has_label_dim,
        }
    }

    /// The expansion query selects PK + each outbound link's FK + the label in
    /// one shot; the plan records where each lands so one scan yields matched,
    /// PK rows, FK values, and the sample.
    #[test]
    fn wm_expansion_plan_lays_out_pk_fk_columns() {
        let meta = meta_for(
            "order",
            "orders",
            vec!["orders.order_id".into()],
            vec![
                link("customer", "orders.customer_id", LinkKind::Parent),
                link("retail_store", "orders.store_id", LinkKind::CrossLink),
            ],
            vec!["orders.order_id".into()],
            false,
        );
        let plan = wm_expansion_plan(&meta);
        assert_eq!(
            plan.dims,
            vec!["orders.order_id", "orders.customer_id", "orders.store_id"]
        );
        assert_eq!(plan.pk_cols, vec![0]);
        assert_eq!(plan.label_col, None);
        assert_eq!(
            plan.link_cols,
            vec![("customer".to_string(), 1), ("retail_store".to_string(), 2)]
        );
    }

    /// When the label dimension coincides with the PK column (cities: `city` is
    /// both key and label), the layout de-duplicates to a single column.
    #[test]
    fn wm_expansion_plan_dedups_label_matching_pk() {
        let meta = meta_for(
            "city",
            "cities",
            vec!["cities.city".into()],
            vec![link("region", "cities.region", LinkKind::Parent)],
            vec!["cities.city".into(), "cities.city".into()],
            true,
        );
        let plan = wm_expansion_plan(&meta);
        assert_eq!(plan.dims, vec!["cities.city", "cities.region"]);
        assert_eq!(plan.pk_cols, vec![0]);
        assert_eq!(plan.label_col, Some(0));
        assert_eq!(plan.link_cols, vec![("region".to_string(), 1)]);
    }

    /// `matched` counts DISTINCT PK tuples (an FK fanning out to duplicate rows
    /// never inflates it); FK values are de-duplicated; the sample is the first
    /// three rows projected to [PK.., label].
    #[test]
    fn parse_expansion_rows_distinct_pk_and_fk() {
        let plan = WmExpansionPlan {
            dims: vec!["v.pk".into(), "v.name".into(), "v.customer_id".into()],
            pk_cols: vec![0],
            label_col: Some(1),
            link_cols: vec![("customer".into(), 2)],
        };
        let rows = vec![
            vec!["1".into(), "Alice".into(), "100".into()],
            vec!["1".into(), "Alice".into(), "100".into()],
            vec!["2".into(), "Bob".into(), "100".into()],
        ];
        let res = parse_expansion_rows(&rows, &plan);
        assert_eq!(res.matched, 2, "distinct PK count, not raw row count");
        assert_eq!(res.pk_rows, vec![vec![s("1")], vec![s("2")]]);
        let (target, mut vals) = res.fk_values.into_iter().next().unwrap();
        assert_eq!(target, "customer");
        vals.sort_by_key(|v| v.as_str().unwrap_or_default().to_string());
        assert_eq!(vals, vec![s("100")]);
        // Sample is deduped by PK tuple: the two identical Alice rows collapse
        // to one preview so a fanned-out FK never wastes a preview slot.
        assert_eq!(res.sample, vec!["Alice", "Bob"]);
        assert_eq!(res.sample_keys, vec!["1", "2"]);
    }

    /// The PK dedup must happen *before* the `take(3)` window, so a leading run
    /// of duplicate rows can't crowd distinct instances out of the 3 preview
    /// slots (the bug: `[1,1,1]` previews instead of `[1,2,3]`).
    #[test]
    fn parse_expansion_rows_sample_dedups_before_take() {
        let plan = WmExpansionPlan {
            dims: vec!["v.pk".into(), "v.name".into()],
            pk_cols: vec![0],
            label_col: Some(1),
            link_cols: vec![],
        };
        let rows = vec![
            vec!["1".into(), "Alice".into()],
            vec!["1".into(), "Alice".into()],
            vec!["1".into(), "Alice".into()],
            vec!["2".into(), "Bob".into()],
            vec!["3".into(), "Carol".into()],
            vec!["4".into(), "Dave".into()],
        ];
        let res = parse_expansion_rows(&rows, &plan);
        assert_eq!(res.sample, vec!["Alice", "Bob", "Carol"]);
        assert_eq!(res.sample_keys, vec!["1", "2", "3"]);
    }

    /// A Foreign entity whose name resolves to no Primary anywhere is not
    /// navigable — it has no node to point at, so it is dropped (mirrors the
    /// `target_exists` guard the drawn-edge builder uses).
    #[test]
    fn build_entity_links_skips_unresolvable_foreign() {
        let orders = wm_view(
            "orders",
            serde_json::json!([
                {"name": "order", "type": "primary", "key": "order_id"},
                {"name": "ghost", "type": "foreign", "key": "ghost_id"},
            ]),
        );
        let views = vec![orders.clone()];
        assert!(build_entity_links(&views, &orders, None).is_empty());
    }

    #[test]
    fn sample_row_single_pk_no_label() {
        let row = vec!["42".to_string()];
        let (display, key) = sample_row_to_display_key(&row, 1, false);
        assert_eq!(display, "42");
        assert_eq!(key, "42");
    }

    #[test]
    fn sample_row_with_label_dim() {
        let row = vec!["42".to_string(), "Acme Corp".to_string()];
        let (display, key) = sample_row_to_display_key(&row, 1, true);
        assert_eq!(display, "Acme Corp");
        assert_eq!(key, "42");
    }

    #[test]
    fn sample_row_composite_pk_json_key() {
        let row = vec!["70978".to_string(), "177411".to_string()];
        let (display, key) = sample_row_to_display_key(&row, 2, false);
        assert_eq!(display, "70978 · 177411");
        assert_eq!(key, r#"["70978","177411"]"#);
    }

    #[test]
    fn sample_row_label_empty_falls_back_to_pks() {
        let row = vec!["42".to_string(), "".to_string()];
        let (display, key) = sample_row_to_display_key(&row, 1, true);
        assert_eq!(display, "42");
        assert_eq!(key, "42");
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
    axum::extract::State(_app_state): axum::extract::State<crate::server::router::AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(q): axum::extract::Query<WmInstancesQuery>,
) -> Result<extract::Json<WmInstancesResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    // Not cached: this is a bounded `SELECT <pk,label> … LIMIT n` scan, cheap
    // enough that caching it isn't worth the staleness risk. A cache keyed on
    // `workspace_id` alone would not invalidate on an out-of-band working-copy
    // change (e.g. `git pull`), serving a previous revision's instances until
    // the TTL lapsed. `is_search` still gates the overflow probe below.
    let is_search = q.search.as_deref().is_some_and(|s| !s.is_empty());

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
    let _table = view
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
    .map(|compiled| match compiled {
        CompiledQuery::Warehouse { sql, database_name } => (sql, database_name),
        CompiledQuery::Preaggregation { preagg_sql, .. } => (preagg_sql, String::new()),
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
    Ok(extract::Json(response))
}

/// Build per-entity drill-down metadata for every Primary entity in the layer.
/// Shared by the filter-counts BFS and the scoped sample-browser endpoint so
/// both traverse the exact same navigable link graph.
fn build_entity_metas(
    layer: &airlayer::SemanticLayer,
    promotions: &Promotions,
    wm_cfg: Option<&super::world_model_config::WorldModelConfig>,
) -> Vec<EntityMeta> {
    let get_display_field = |entity_id: &str| -> Option<String> {
        wm_cfg
            .and_then(|cfg| cfg.entities.iter().find(|e| e.id == entity_id))
            .and_then(|ec| ec.display_field.clone())
    };
    layer
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
            let parent_entity = promotions.parent_of(&primary.name);
            let links = build_entity_links(&layer.views, view, parent_entity);
            let disp = EntityDisplaySpec::for_entity(
                view,
                &primary.name,
                get_display_field(&primary.name).as_deref(),
            );
            Some(EntityMeta {
                entity_name: primary.name.clone(),
                view_name: view.name.clone(),
                datasource: view.datasource.clone().unwrap_or_default(),
                pk_dim_refs,
                links,
                sample_dims: disp.dims,
                pk_count: disp.pk_count,
                has_label_dim: disp.has_label_dim,
            })
        })
        .collect()
}

/// Everything needed to compile + execute one drill-down query outside the
/// streaming filter-counts handler. Compilation uses `resolve_and_compile` (no
/// engine cache) — the sample-browser is an on-demand, low-QPS surface where the
/// simpler path is fine.
struct WmExecCtx {
    workspace_manager: WorkspaceManager,
    user_id: Uuid,
    role: WorkspaceRole,
    scan_path: std::path::PathBuf,
    databases: Vec<airlayer::DatabaseConfig>,
    layer: airlayer::SemanticLayer,
}

impl WmExecCtx {
    /// Compile a config to `(sql, database_name)`; `None` on compile failure.
    async fn compile_full(&self, cfg: SemanticQueryConfig) -> Option<(String, String)> {
        let sp = self.scan_path.clone();
        let dbs = self.databases.clone();
        let layer = self.layer.clone();
        tokio::task::spawn_blocking(move || {
            resolve_and_compile(&sp, &dbs, &cfg, None, 0, Some(layer)).ok()
        })
        .await
        .ok()
        .flatten()
        .map(|compiled| match compiled {
            CompiledQuery::Warehouse { sql, database_name } => (sql, database_name),
            CompiledQuery::Preaggregation { preagg_sql, .. } => (preagg_sql, String::new()),
        })
    }

    /// Run one expansion query and parse it into matched PK rows + outbound FK
    /// values (the fuel the BFS needs to reach the next hop). Empty on any
    /// failure so an unreachable node just contributes nothing.
    async fn run_expansion(
        &self,
        datasource: &str,
        cfg: SemanticQueryConfig,
        plan: &WmExpansionPlan,
    ) -> WmExpansionResult {
        let empty = || WmExpansionResult {
            matched: 0,
            pk_rows: vec![],
            fk_values: vec![],
            sample: vec![],
            sample_keys: vec![],
        };
        let Some((sql, _db)) = self.compile_full(cfg).await else {
            return empty();
        };
        let Ok(connector) = build_connector(
            &self.workspace_manager,
            self.user_id,
            self.role.clone(),
            datasource,
        )
        .await
        else {
            return empty();
        };
        let rows = run_with_connector(&connector, &sql, &self.workspace_manager).await;
        parse_expansion_rows(&rows, plan)
    }
}

/// Build the single-column `Eq` PK filter that selects one entity's own instance.
/// Parse a request key into its PK component values. The picker encodes a
/// composite-PK instance as a JSON array of strings (`["2","4"]`); a single-PK
/// instance may arrive either as a bare scalar or a one-element array. Mirrors
/// the instance-detail / measure-breakdown handlers so every consumer agrees on
/// the same decoding.
fn parse_key_values(key: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(key).unwrap_or_else(|_| vec![key.to_string()])
}

/// The seed's PK as a single composite row (`Vec<Value>`, one entry per PK
/// column). Used to seed BFS `pk_rows` when the pre-fetch query returns nothing.
fn seed_pk_row(key: &str) -> Vec<serde_json::Value> {
    parse_key_values(key)
        .into_iter()
        .map(serde_json::Value::String)
        .collect()
}

/// Equality filters selecting the seed instance by its PK. A single value maps
/// to the first PK dim (the picker's single-PK flow, unchanged); multiple values
/// are zipped positionally against the composite PK dims — so an `order_item`
/// keyed by `["2","4"]` filters `order_id=2 AND line=4` rather than comparing the
/// first PK column against the literal JSON string `["2","4"]` (which matches
/// nothing and zeroed every reachable count). Empty only when the entity has no
/// PK dims.
fn seed_self_filters(meta: &EntityMeta, key: &str) -> Vec<SemanticFilter> {
    let values = parse_key_values(key);
    let mk = |field: String, value: String| SemanticFilter {
        field,
        filter_type: SemanticFilterType::Eq(ScalarFilter {
            value: serde_json::Value::String(value),
        }),
    };
    if values.len() == 1 {
        return meta
            .pk_dim_refs
            .first()
            .cloned()
            .map(|field| vec![mk(field, values[0].clone())])
            .unwrap_or_default();
    }
    meta.pk_dim_refs
        .iter()
        .zip(values.iter())
        .map(|(field, val)| mk(field.clone(), val.clone()))
        .collect()
}

#[cfg(test)]
mod seed_key_tests {
    use super::*;

    fn meta(view: &str, pk_dims: &[&str]) -> EntityMeta {
        EntityMeta {
            entity_name: view.to_string(),
            view_name: view.to_string(),
            datasource: "db".to_string(),
            pk_dim_refs: pk_dims.iter().map(|d| format!("{view}.{d}")).collect(),
            links: vec![],
            sample_dims: vec![],
            pk_count: pk_dims.len(),
            has_label_dim: false,
        }
    }

    fn eq_value(f: &SemanticFilter) -> &str {
        match &f.filter_type {
            SemanticFilterType::Eq(ScalarFilter {
                value: serde_json::Value::String(s),
            }) => s,
            _ => panic!("expected string Eq filter"),
        }
    }

    #[test]
    fn scalar_key_filters_first_pk_dim() {
        let m = meta("order_item", &["id"]);
        let filters = seed_self_filters(&m, "42");
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].field, "order_item.id");
        assert_eq!(eq_value(&filters[0]), "42");
    }

    // The regression: a composite-PK seed encoded as a JSON array must filter
    // every PK column, not compare the first column to the literal `["2","4"]`.
    #[test]
    fn composite_json_key_filters_each_pk_dim() {
        let m = meta("order_item", &["order_id", "line_number"]);
        let filters = seed_self_filters(&m, "[\"2\",\"4\"]");
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].field, "order_item.order_id");
        assert_eq!(eq_value(&filters[0]), "2");
        assert_eq!(filters[1].field, "order_item.line_number");
        assert_eq!(eq_value(&filters[1]), "4");
    }

    // A single-element JSON array is still the single-PK flow (first dim only).
    #[test]
    fn single_element_array_maps_to_first_pk_dim() {
        let m = meta("order_item", &["id"]);
        let filters = seed_self_filters(&m, "[\"7\"]");
        assert_eq!(filters.len(), 1);
        assert_eq!(eq_value(&filters[0]), "7");
    }

    #[test]
    fn no_pk_dims_yields_no_filters() {
        let m = meta("order_item", &[]);
        assert!(seed_self_filters(&m, "42").is_empty());
    }

    #[test]
    fn seed_pk_row_expands_composite_key() {
        assert_eq!(
            seed_pk_row("[\"2\",\"4\"]"),
            vec![
                serde_json::Value::String("2".into()),
                serde_json::Value::String("4".into()),
            ]
        );
        assert_eq!(
            seed_pk_row("42"),
            vec![serde_json::Value::String("42".into())]
        );
    }
}

/// Local BFS state fuel — an entity's matched PK rows and its distinct outbound
/// FK values per target (identical role to the filter-counts `NeighborData`).
struct BfsNeighbor {
    pk_rows: Vec<Vec<serde_json::Value>>,
    fk_values: HashMap<String, Vec<serde_json::Value>>,
}

/// Reconstruct the single-view `SemanticFilter` set that selects the rows of
/// `target` reachable from the seed instance, replaying the same undirected
/// link-graph BFS `filter-counts` uses — but stopping the moment `target` is
/// discovered and returning the filters that reach it. `None` when `target` is
/// unreachable from the seed.
///
/// The returned filters constrain only `target`'s own view (its PK for an
/// outbound/coarser target, its FK for an inbound/finer one), so the caller can
/// drop them straight into a paginated `SELECT` over that entity.
async fn resolve_target_filters(
    exec: &WmExecCtx,
    entity_metas: &[EntityMeta],
    seed_entity: &str,
    seed_key: &str,
    target: &str,
) -> Option<Vec<SemanticFilter>> {
    let meta_idx: HashMap<&str, usize> = entity_metas
        .iter()
        .enumerate()
        .map(|(i, m)| (m.entity_name.as_str(), i))
        .collect();
    let seed_idx = *meta_idx.get(seed_entity)?;
    let target_idx = *meta_idx.get(target)?;

    // The seed itself: match its own PK (no traversal needed).
    if seed_entity == target {
        let filters = seed_self_filters(&entity_metas[seed_idx], seed_key);
        return (!filters.is_empty()).then_some(filters);
    }

    // Reverse adjacency: coarser entity → its finer neighbours (child idx, the
    // child's FK refs pointing at it) — the inbound direction cross-links need.
    let inbound = build_inbound_index(entity_metas);

    // Schema-level reachability (pure, no IO) — see the identical reasoning in
    // `post_world_model_filter_counts`. `target` unreachable in the schema
    // graph at all ⇒ nothing more to show, same as an empty BFS today.
    let reachable = schema_reachable_entities(entity_metas, &inbound, &meta_idx, seed_idx);
    if !reachable.contains(&target_idx) {
        return None;
    }

    // Fast path: same datasource ⇒ a filter on the seed's own view is enough.
    // airlayer auto-joins `target`'s view back to the seed's, however many
    // hops apart, when the caller's query later references both — no BFS
    // needed to compute the filter set at all.
    if entity_metas[target_idx].datasource == entity_metas[seed_idx].datasource {
        let filters = seed_self_filters(&entity_metas[seed_idx], seed_key);
        return (!filters.is_empty()).then_some(filters);
    }

    // Legacy fallback: cross-datasource pair — airlayer can't join across
    // datasources, so thread matched values through as literal filters,
    // hop by hop, same as before.
    //
    // Seed pre-fetch — learn its PK rows + outbound FK values so hop 1 can expand.
    let mut neighbor_data: HashMap<String, BfsNeighbor> = HashMap::new();
    let seed_meta = &entity_metas[seed_idx];
    if seed_meta.links.is_empty() {
        neighbor_data.insert(
            seed_entity.to_string(),
            BfsNeighbor {
                pk_rows: vec![seed_pk_row(seed_key)],
                fk_values: HashMap::new(),
            },
        );
    } else {
        let plan = wm_expansion_plan(seed_meta);
        let cfg = SemanticQueryConfig {
            topic: None,
            dimensions: plan.dims.clone(),
            measures: vec![],
            time_dimensions: vec![],
            filters: seed_self_filters(seed_meta, seed_key),
            orders: vec![],
            limit: None,
            offset: None,
        };
        let res = exec.run_expansion(&seed_meta.datasource, cfg, &plan).await;
        let pk_rows = if res.pk_rows.is_empty() {
            vec![seed_pk_row(seed_key)]
        } else {
            res.pk_rows
        };
        neighbor_data.insert(
            seed_entity.to_string(),
            BfsNeighbor {
                pk_rows,
                fk_values: res.fk_values.into_iter().collect(),
            },
        );
    }

    let mut visited: std::collections::HashSet<String> =
        std::collections::HashSet::from([seed_entity.to_string()]);
    let mut frontier: Vec<String> = vec![seed_entity.to_string()];

    while !frontier.is_empty() {
        // Assemble the filter set for every newly reachable entity (no IO here —
        // each frontier entity already carries the fuel it needs).
        let chosen = assemble_hop_filters(
            entity_metas,
            &meta_idx,
            &inbound,
            &neighbor_data,
            &frontier,
            &visited,
        );
        if chosen.is_empty() {
            break;
        }

        // Target reached — its filter set is exactly what selects its reachable
        // rows. Return before running target's own (possibly large) expansion.
        if let Some(&t_idx) = meta_idx.get(target)
            && let Some(filters) = chosen.get(&t_idx)
        {
            return Some(filters.clone());
        }

        // Expand every discovered node to fuel the next hop.
        let mut next_frontier: Vec<String> = Vec::new();
        for (idx, filters) in chosen {
            let meta = &entity_metas[idx];
            visited.insert(meta.entity_name.clone());
            let plan = wm_expansion_plan(meta);
            let cfg = SemanticQueryConfig {
                topic: None,
                dimensions: plan.dims.clone(),
                measures: vec![],
                time_dimensions: vec![],
                filters,
                orders: vec![],
                limit: None,
                offset: None,
            };
            let res = exec.run_expansion(&meta.datasource, cfg, &plan).await;
            if res.matched > 0 {
                neighbor_data.insert(
                    meta.entity_name.clone(),
                    BfsNeighbor {
                        pk_rows: res.pk_rows,
                        fk_values: res.fk_values.into_iter().collect(),
                    },
                );
                next_frontier.push(meta.entity_name.clone());
            }
        }
        frontier = next_frontier;
    }
    None
}

/// One BFS hop's filter assembly (pure): for every entity newly reachable from
/// `frontier`, produce the `SemanticFilter` set that selects its reachable rows.
/// First-writer-wins per entity so a diamond within a level yields one filter.
fn assemble_hop_filters<'a>(
    entity_metas: &'a [EntityMeta],
    meta_idx: &HashMap<&'a str, usize>,
    inbound: &HashMap<&'a str, Vec<(usize, &'a [String])>>,
    neighbor_data: &HashMap<String, BfsNeighbor>,
    frontier: &[String],
    visited: &std::collections::HashSet<String>,
) -> HashMap<usize, Vec<SemanticFilter>> {
    let mut chosen: HashMap<usize, Vec<SemanticFilter>> = HashMap::new();
    for e_name in frontier {
        let Some(nd) = neighbor_data.get(e_name) else {
            continue;
        };
        if nd.pk_rows.is_empty() {
            continue;
        }
        // Inbound (finer): children whose FK points at this frontier entity.
        if let Some(children) = inbound.get(e_name.as_str()) {
            for &(child_idx, fk_refs) in children {
                let child_name = &entity_metas[child_idx].entity_name;
                if visited.contains(child_name) || chosen.contains_key(&child_idx) {
                    continue;
                }
                let f = child_fk_filters(fk_refs, &nd.pk_rows);
                if !f.is_empty() {
                    chosen.insert(child_idx, f);
                }
            }
        }
        // Outbound (coarser): targets filtered by this entity's FK values.
        let Some(&e_idx) = meta_idx.get(e_name.as_str()) else {
            continue;
        };
        for link in &entity_metas[e_idx].links {
            let Some(&t_idx) = meta_idx.get(link.target_entity.as_str()) else {
                continue;
            };
            if visited.contains(&link.target_entity) || chosen.contains_key(&t_idx) {
                continue;
            }
            let Some(values) = nd.fk_values.get(&link.target_entity) else {
                continue;
            };
            if values.is_empty() {
                continue;
            }
            let Some(pk_field) = entity_metas[t_idx].pk_dim_refs.first().cloned() else {
                continue;
            };
            chosen.insert(
                t_idx,
                vec![SemanticFilter {
                    field: pk_field,
                    filter_type: SemanticFilterType::In(ArrayFilter {
                        values: values.clone(),
                    }),
                }],
            );
        }
    }
    chosen
}

#[derive(Deserialize)]
pub struct WmFilterInstancesQuery {
    /// The active/selected instance's entity (the filter seed).
    pub seed_entity: String,
    /// The active instance's key.
    pub seed_key: String,
    /// Target entity whose reachable rows to list.
    pub entity: String,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

/// `GET /{workspace_id}/semantic/world-model/filter-instances`
///
/// Paginated, searchable listing of the rows of `entity` reachable from the
/// selected instance (`seed_entity` / `seed_key`) — the full set the node card
/// only previews as a handful of sample chips. Backs the "+N more" sample
/// browser popover.
pub async fn get_world_model_filter_instances(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    axum::extract::State(_app_state): axum::extract::State<crate::server::router::AppState>,
    Path(WorkspacePath { workspace_id }): Path<WorkspacePath>,
    axum::extract::Query(q): axum::extract::Query<WmFilterInstancesQuery>,
) -> Result<extract::Json<WmInstancesResponse>, (StatusCode, extract::Json<ErrorResponse>)> {
    let err = |code: StatusCode, message: String| (code, extract::Json(ErrorResponse { message }));

    let semantics_path = workspace_manager.config_manager.semantics_scan_path();
    let layer = layer_cache.get_or_load(semantics_path).await.map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load layer: {e}"),
        )
    })?;
    let promotions = Promotions::build(&layer.views)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let wm_cfg = super::world_model_config::WorldModelConfig::resolve(
        workspace_id,
        workspace_manager.config_manager.workspace_path(),
    )
    .await
    .ok()
    .flatten();

    let entity_metas = build_entity_metas(&layer, &promotions, wm_cfg.as_ref());

    let target_view = primary_view_of(&layer, &q.entity).ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            format!("Entity '{}' not found", q.entity),
        )
    })?;
    let display_field = wm_cfg
        .as_ref()
        .and_then(|cfg| cfg.entities.iter().find(|e| e.id == q.entity))
        .and_then(|ec| ec.display_field.clone());
    let disp = EntityDisplaySpec::for_entity(target_view, &q.entity, display_field.as_deref());
    if disp.dims.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            format!("Entity '{}' has no key columns", q.entity),
        ));
    }

    let databases: Vec<airlayer::DatabaseConfig> = workspace_manager
        .config_manager
        .list_databases()
        .iter()
        .map(|db| airlayer::DatabaseConfig {
            name: db.name.clone(),
            db_type: db.database_type.to_string(),
        })
        .collect();
    let exec = WmExecCtx {
        workspace_manager: workspace_manager.clone(),
        user_id: user.id,
        role: role.clone(),
        scan_path: workspace_manager.config_manager.semantics_scan_path(),
        databases,
        layer: (*layer).clone(),
    };

    // Resolve the reachability filter (seed instance → target entity). Unreachable
    // ⇒ empty page rather than an error (the node card simply had nothing more).
    let Some(mut filters) =
        resolve_target_filters(&exec, &entity_metas, &q.seed_entity, &q.seed_key, &q.entity).await
    else {
        return Ok(extract::Json(WmInstancesResponse {
            total: 0,
            has_more: false,
            items: vec![],
        }));
    };

    // Optional search — Contains on the label dim, Eq on a bare PK (mirrors the
    // instance-picker endpoint).
    if let Some(term) = q.search.as_deref().filter(|s| !s.is_empty()) {
        let (field, op) = if disp.has_label_dim {
            (
                disp.dims[disp.pk_count].clone(),
                SemanticFilterType::Contains(ScalarFilter { value: term.into() }),
            )
        } else {
            (
                disp.dims.first().cloned().unwrap_or_default(),
                SemanticFilterType::Eq(ScalarFilter { value: term.into() }),
            )
        };
        filters.push(SemanticFilter {
            field,
            filter_type: op,
        });
    }

    let order_by = disp.dims.first().cloned().unwrap_or_default();
    // Overflow probe: fetch limit+1 (with offset) to detect a next page.
    let cfg = SemanticQueryConfig {
        topic: None,
        dimensions: disp.dims.clone(),
        measures: vec![],
        time_dimensions: vec![],
        filters,
        orders: vec![SemanticOrder {
            field: order_by,
            direction: "asc".to_string(),
        }],
        limit: Some((q.limit as u64) + 1),
        offset: Some(q.offset as u64),
    };

    let (sql, database_name) = exec.compile_full(cfg).await.ok_or_else(|| {
        err(
            StatusCode::BAD_REQUEST,
            "failed to compile query".to_string(),
        )
    })?;

    let payload = SQLParams {
        sql,
        database: database_name,
        filters: None,
        connections: None,
        result_format: None,
    };
    let rows = match run_via_agentic_connector(&workspace_manager, user.id, role, &payload).await {
        Ok(SemanticQueryResponse::Json(r)) => r,
        _ => vec![],
    };

    let mut all_items: Vec<WmInstanceItem> = rows
        .into_iter()
        .skip(1) // header row
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

    let has_more = all_items.len() > q.limit;
    all_items.truncate(q.limit);
    let total = all_items.len();

    Ok(extract::Json(WmInstancesResponse {
        total,
        has_more,
        items: all_items,
    }))
}

/// `POST /{workspace_id}/semantic/world-model/filter-counts`
pub async fn post_world_model_filter_counts(
    WorkspaceManagerExtractor(workspace_manager): WorkspaceManagerExtractor,
    AuthenticatedUserExtractor(user): AuthenticatedUserExtractor,
    EffectiveWorkspaceRole(role): EffectiveWorkspaceRole,
    layer_cache: SemanticLayerCacheCtx,
    engine_cache: SemanticEngineCacheCtx,
    axum::extract::State(_app_state): axum::extract::State<crate::server::router::AppState>,
    Path(WorkspacePath { workspace_id: _ }): Path<WorkspacePath>,
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

    // World-model config supplies per-entity display fields used to render sample
    // labels on descendant cards (mirrors the instance-detail handler).
    let wm_cfg = super::world_model_config::WorldModelConfig::resolve(
        layer_cache.workspace_id,
        workspace_manager.config_manager.workspace_path(),
    )
    .await
    .ok()
    .flatten();

    // Collect per-entity metadata needed to build semantic queries (struct
    // hoisted to module scope so the expansion-plan helpers can share it).
    let entity_metas: Vec<EntityMeta> = build_entity_metas(&layer, &promotions, wm_cfg.as_ref());

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
                let sqls: Vec<Option<String>> = cfgs.iter().map(compile_one).collect();
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
                    let sent = tx_a
                        .send(WmFilterCountEvent {
                            entity_name: name,
                            total: Some(total),
                            matched: None,
                            sample: vec![],
                            sample_keys: vec![],
                            done: false,
                        })
                        .await;
                    if sent.is_err() {
                        // Receiver dropped — client disconnected (e.g. re-filtered
                        // before this stream finished). Stop draining `futs`;
                        // dropping it below cancels any still-in-flight queries
                        // instead of letting them run to completion unread.
                        tracing::debug!(
                            "filter-counts: total-count receiver dropped, stopping early"
                        );
                        break;
                    }
                }
                tracing::info!(
                    elapsed_ms = t_exec.elapsed().as_millis(),
                    n,
                    "filter-counts: total counts streamed"
                );
            },
            // ── Task B: undirected BFS over the entity link graph ─────────────
            //
            // Instance drill-down follows the *navigable link graph* — the parent
            // spine PLUS foreign cross-links, i.e. exactly the edges drawn in the
            // graph — not just the parent tree. From the selected instance we
            // expand in BOTH directions, hop by hop:
            //
            //   • coarser (outbound): the current entity's own FK → the target's
            //     PK (store → city, store → region);
            //   • finer (inbound): entities whose FK points AT the current entity
            //     (store ← order — the cross-link case the parent tree missed).
            //
            // Each hop carries the accumulated PK-filter set. A visited set keyed
            // on entity (not path) makes every entity count exactly once and
            // guards against cycles and diamonds. BFS hop distance from the seed
            // defines the streaming order (still one SSE burst per level).
            async move {
                // Reconstruct batch_compile inside this block so it's owned
                // (no reference to outer function stack after tokio::spawn).
                // Compile a whole batch under a SINGLE engine acquisition. The
                // cached engine is `Send + !Sync` behind a `Mutex`; locking once
                // per batch (instead of once per query) keeps compilation — which
                // is on the BFS critical path — from paying repeated lock churn,
                // and builds the fallback engine at most once per batch.
                let batch_compile = |cfgs: Vec<SemanticQueryConfig>| {
                    let engine_arc = cached_engine.clone();
                    let layer_c = layer_inner.clone();
                    let dbs_c = databases.clone();
                    tokio::task::spawn_blocking(move || {
                        let compile_all = |engine: &_| -> Vec<Option<String>> {
                            cfgs.iter()
                                .map(|cfg| agentic_semantic::compile_with_engine(engine, cfg).ok())
                                .collect()
                        };
                        let sqls: Vec<Option<String>> = if let Some(ref arc) = engine_arc {
                            match arc.lock() {
                                Ok(engine) => compile_all(&engine),
                                Err(_) => vec![None; cfgs.len()],
                            }
                        } else {
                            let dialects =
                                airlayer::DatasourceDialectMap::from_config_databases(&dbs_c);
                            match airlayer::SemanticEngine::from_semantic_layer(layer_c, dialects) {
                                Ok(engine) => compile_all(&engine),
                                Err(_) => vec![None; cfgs.len()],
                            }
                        };
                        Ok::<_, agentic_semantic::SemanticError>(sqls)
                    })
                };

                // Seed matched = 1 (the record itself) — emit immediately.
                tx_b.send(WmFilterCountEvent {
                    entity_name: req.entity_id.clone(),
                    matched: Some(1),
                    total: None,
                    sample: vec![],
                    sample_keys: vec![],
                    done: false,
                })
                .await
                .ok();

                // Everything a BFS hop needs to expand FROM an entity, produced by
                // that entity's single expansion query: its matched PK rows (each a
                // tuple of column values, one per `pk_dim_ref`; composite keys keep
                // all columns so per-column IN filters stay correct) and, per
                // outbound link, the distinct FK values pointing at the target.
                struct NeighborData {
                    pk_rows: Vec<Vec<serde_json::Value>>,
                    /// target entity → distinct FK values on this entity's side.
                    fk_values: HashMap<String, Vec<serde_json::Value>>,
                }
                let mut neighbor_data: HashMap<String, NeighborData> = HashMap::new();

                // entity_name → index into entity_metas, for O(1) neighbour lookup.
                let meta_idx: HashMap<&str, usize> = entity_metas
                    .iter()
                    .enumerate()
                    .map(|(i, m)| (m.entity_name.as_str(), i))
                    .collect();

                // Reverse adjacency: target entity → its *finer* neighbours (the
                // inbound direction the parent tree never provided for
                // cross-links, e.g. store ← order). Shared helper — also used
                // by `resolve_target_filters`'s legacy fallback.
                let inbound = build_inbound_index(&entity_metas);

                // Schema-level reachability (pure, no IO) decides which path
                // this request takes. Every entity pair the world model draws
                // an edge for is automatically joinable by airlayer — its join
                // graph is derived from the exact same FK/PK entity metadata
                // this file already reads to build `EntityLink` — UNLESS the
                // pair spans two different datasources, since airlayer rejects
                // cross-dialect joins. So: when every reachable entity shares
                // the seed's datasource, skip the BFS entirely and fire one
                // direct-join count+sample query per entity — no per-hop
                // materialization, no unbounded expansion query. Otherwise
                // fall back to the legacy per-hop BFS below, which threads
                // matched values through as literal `IN (...)` filters and
                // stays correct across datasources.
                let seed_idx_opt = meta_idx.get(req.entity_id.as_str()).copied();
                let (reachable, any_cross_datasource) = match seed_idx_opt {
                    Some(seed_idx) => {
                        let reachable =
                            schema_reachable_entities(&entity_metas, &inbound, &meta_idx, seed_idx);
                        let seed_ds = entity_metas[seed_idx].datasource.clone();
                        let cross = reachable
                            .iter()
                            .any(|&i| entity_metas[i].datasource != seed_ds);
                        (reachable, cross)
                    }
                    None => (vec![], false),
                };

                if let Some(seed_idx) = seed_idx_opt.filter(|_| !any_cross_datasource) {
                    // ── Fast path: direct join back to the seed ───────────────
                    //
                    // A single query referencing the target entity's own view
                    // (measures/dimensions) plus a filter on the seed's view
                    // lets airlayer resolve the *entire* join chain back to
                    // the seed automatically — however many hops apart.
                    //
                    // A real scalar `COUNT(*)` measure query — no row
                    // fetching, no client-side dedup — now that
                    // github.com/oxy-hq/airlayer@64163f5 fixed the bug where
                    // its fan-out-protection CTE builder derived its join
                    // scope only from `dimensions`, ignoring `filters`
                    // entirely: a filter on a view other than the measure's
                    // own (this seed filter, exactly) was silently never
                    // joined into the CTE, so the aggregate could evaluate
                    // over the wrong (or no) scope. Before that fix this had
                    // to be worked around with a dimension-only projection +
                    // client-side row fetch/dedup, which reintroduced the
                    // unbounded-memory risk this whole design set out to
                    // avoid. See [[airlayer-fanout-protection-zero-dim-bug]]
                    // in project memory for the full history.
                    let filters = seed_self_filters(&entity_metas[seed_idx], &req.key_value);
                    if filters.is_empty() {
                        return;
                    }

                    struct DirectWork<'a> {
                        meta: &'a EntityMeta,
                        count_cfg: SemanticQueryConfig,
                        sample_cfg: Option<SemanticQueryConfig>,
                    }
                    let works: Vec<DirectWork> = reachable
                        .iter()
                        .map(|&idx| {
                            let meta = &entity_metas[idx];
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
                            let sample_cfg =
                                (!meta.sample_dims.is_empty()).then(|| SemanticQueryConfig {
                                    topic: None,
                                    dimensions: meta.sample_dims.clone(),
                                    measures: vec![],
                                    time_dimensions: vec![],
                                    filters: filters.clone(),
                                    // Order ascending on the first sample dim so preview
                                    // chips are stable across reloads and align with the
                                    // ascending Sample Browser (`get_world_model_filter_instances`).
                                    orders: vec![SemanticOrder {
                                        field: meta.sample_dims[0].clone(),
                                        direction: "asc".to_string(),
                                    }],
                                    limit: Some(3),
                                    offset: None,
                                });
                            DirectWork {
                                meta,
                                count_cfg,
                                sample_cfg,
                            }
                        })
                        .collect();

                    let all_cfgs: Vec<SemanticQueryConfig> = works
                        .iter()
                        .flat_map(|w| {
                            let mut v = vec![w.count_cfg.clone()];
                            if let Some(ref s) = w.sample_cfg {
                                v.push(s.clone());
                            }
                            v
                        })
                        .collect();

                    let t_compile = std::time::Instant::now();
                    let all_sqls: Vec<Option<String>> = batch_compile(all_cfgs)
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .unwrap_or_else(|| {
                            vec![
                                None;
                                works
                                    .iter()
                                    .map(|w| 1 + w.sample_cfg.is_some() as usize)
                                    .sum()
                            ]
                        });
                    tracing::info!(
                        elapsed_ms = t_compile.elapsed().as_millis(),
                        n = works.len(),
                        "filter-counts direct-join: compiled queries"
                    );

                    let mut sql_iter = all_sqls.into_iter();
                    let exec_futures: Vec<_> = works
                        .into_iter()
                        .map(|w| {
                            let count_sql = sql_iter.next().flatten();
                            let sample_sql = w
                                .sample_cfg
                                .as_ref()
                                .and_then(|_| sql_iter.next().flatten());
                            let datasource = w.meta.datasource.clone();
                            let entity_name = w.meta.entity_name.clone();
                            let pk_count = w.meta.pk_count;
                            let has_label_dim = w.meta.has_label_dim;
                            let wm = wm_b.clone();
                            let role_c = role_b.clone();
                            async move {
                                let connector = build_connector(&wm, user_id, role_c, &datasource)
                                    .await
                                    .ok();
                                let (matched, (sample, sample_keys)) = match connector.as_ref() {
                                    Some(c) => tokio::join!(
                                        async {
                                            match count_sql {
                                                Some(sql) => run_with_connector(c, &sql, &wm)
                                                    .await
                                                    .into_iter()
                                                    .next()
                                                    .and_then(|r| r.into_iter().next())
                                                    .and_then(|v: String| v.parse::<u64>().ok())
                                                    .unwrap_or(0),
                                                None => 0,
                                            }
                                        },
                                        async {
                                            match sample_sql {
                                                Some(sql) => run_with_connector(c, &sql, &wm)
                                                    .await
                                                    .into_iter()
                                                    .map(|r| {
                                                        sample_row_to_display_key(
                                                            &r,
                                                            pk_count,
                                                            has_label_dim,
                                                        )
                                                    })
                                                    .unzip(),
                                                None => (vec![], vec![]),
                                            }
                                        },
                                    ),
                                    None => (0, (vec![], vec![])),
                                };
                                (entity_name, matched, sample, sample_keys)
                            }
                        })
                        .collect();

                    let t_exec = std::time::Instant::now();
                    let mut futs: FuturesUnordered<_> = exec_futures.into_iter().collect();
                    let mut n = 0usize;
                    while let Some((entity_name, matched, sample, sample_keys)) = futs.next().await
                    {
                        n += 1;
                        let sent = tx_b
                            .send(WmFilterCountEvent {
                                entity_name,
                                matched: Some(matched),
                                total: None,
                                sample,
                                sample_keys,
                                done: false,
                            })
                            .await;
                        if sent.is_err() {
                            tracing::debug!(
                                "filter-counts direct-join: receiver dropped, stopping early"
                            );
                            break;
                        }
                    }
                    tracing::info!(
                        elapsed_ms = t_exec.elapsed().as_millis(),
                        n,
                        "filter-counts direct-join: all queries done"
                    );
                    return;
                }

                // ── Legacy fallback: per-hop BFS with IN(...) threading ───────
                //
                // Used when the seed entity is unknown to the model, or when
                // the reachable set spans more than one datasource (airlayer
                // cannot join across datasources, so the direct-join fast path
                // above is unavailable and matched values must be threaded
                // through as literal filters instead).
                //
                // Whether an entity has any onward neighbour to expand into — an
                // outbound link or an inbound child. Only such entities run the
                // full (all-rows) expansion query; terminal nodes get a cheap
                // scalar count + limited sample instead.
                let is_expandable = |name: &str| -> bool {
                    meta_idx
                        .get(name)
                        .is_some_and(|&i| !entity_metas[i].links.is_empty())
                        || inbound.contains_key(name)
                };

                // Run one entity's compiled expansion `sql` and parse it. Empty
                // result on any failure so the BFS just treats the node as unmatched.
                let run_expansion = |datasource: String,
                                     sql: Option<String>,
                                     plan: WmExpansionPlan| {
                    let wm = wm_b.clone();
                    let role = role_b.clone();
                    async move {
                        let empty = || WmExpansionResult {
                            matched: 0,
                            pk_rows: vec![],
                            fk_values: vec![],
                            sample: vec![],
                            sample_keys: vec![],
                        };
                        let Some(sql) = sql else { return empty() };
                        let Ok(connector) = build_connector(&wm, user_id, role, &datasource).await
                        else {
                            return empty();
                        };
                        let rows = run_with_connector(&connector, &sql, &wm).await;
                        parse_expansion_rows(&rows, &plan)
                    }
                };

                // ── Seed pre-fetch: learn the seed's outbound FK values (and PK
                // rows) so the first hop can expand from it. A seed with no links
                // only needs its PK, which we already have (the picked key).
                if is_expandable(&req.entity_id)
                    && let Some(&i) = meta_idx.get(req.entity_id.as_str())
                    && !entity_metas[i].links.is_empty()
                {
                    let meta = &entity_metas[i];
                    let plan = wm_expansion_plan(meta);
                    let seed_cfg = SemanticQueryConfig {
                        topic: None,
                        dimensions: plan.dims.clone(),
                        measures: vec![],
                        time_dimensions: vec![],
                        filters: seed_self_filters(meta, &req.key_value),
                        orders: vec![],
                        limit: None,
                        offset: None,
                    };
                    let sql = batch_compile(vec![seed_cfg])
                        .await
                        .ok()
                        .and_then(|r| r.ok())
                        .and_then(|mut v| v.pop())
                        .flatten();
                    let res = run_expansion(meta.datasource.clone(), sql, plan).await;
                    let pk_rows = if res.pk_rows.is_empty() {
                        vec![seed_pk_row(&req.key_value)]
                    } else {
                        res.pk_rows
                    };
                    neighbor_data.insert(
                        req.entity_id.clone(),
                        NeighborData {
                            pk_rows,
                            fk_values: res.fk_values.into_iter().collect(),
                        },
                    );
                } else {
                    neighbor_data.insert(
                        req.entity_id.clone(),
                        NeighborData {
                            pk_rows: vec![seed_pk_row(&req.key_value)],
                            fk_values: HashMap::new(),
                        },
                    );
                }

                let mut visited: std::collections::HashSet<String> =
                    std::collections::HashSet::from([req.entity_id.clone()]);
                let mut frontier: Vec<String> = vec![req.entity_id.clone()];
                let mut depth = 0usize;
                let t_bfs = std::time::Instant::now();

                // BFS: expand the frontier one hop at a time until nothing new is
                // reachable. Each iteration discovers the next ring of entities and
                // emits one SSE burst for them (keeps the progressive-reveal UX).
                while !frontier.is_empty() {
                    depth += 1;

                    // ── Assemble filters for every newly discovered entity ────────
                    // No queries here — each frontier entity already carries what it
                    // needs to expand (its `NeighborData`, produced by its own
                    // expansion query at the previous level / seed pre-fetch):
                    //   • inbound (finer) children resolve via `child_fk_filters`
                    //     over the frontier entity's PK rows;
                    //   • outbound (coarser) targets use the frontier entity's
                    //     pre-resolved FK values as an `IN` filter on the target PK.
                    // First-writer wins per entity so a diamond within one level
                    // still yields a single count.
                    let mut chosen: HashMap<usize, Vec<agentic_semantic::config::SemanticFilter>> =
                        HashMap::new();
                    for e_name in &frontier {
                        let Some(nd) = neighbor_data.get(e_name) else {
                            continue;
                        };
                        if nd.pk_rows.is_empty() {
                            continue;
                        }
                        // Inbound: children whose FK points at this frontier entity.
                        if let Some(children) = inbound.get(e_name.as_str()) {
                            for &(child_idx, fk_refs) in children {
                                let child_name = &entity_metas[child_idx].entity_name;
                                if visited.contains(child_name) || chosen.contains_key(&child_idx) {
                                    continue;
                                }
                                let f = child_fk_filters(fk_refs, &nd.pk_rows);
                                if f.is_empty() {
                                    continue;
                                }
                                chosen.insert(child_idx, f);
                            }
                        }
                        // Outbound: coarser targets, filtered by the pre-resolved FK
                        // values (no per-level FK-select query anymore).
                        let Some(&e_idx) = meta_idx.get(e_name.as_str()) else {
                            continue;
                        };
                        for link in &entity_metas[e_idx].links {
                            let Some(&t_idx) = meta_idx.get(link.target_entity.as_str()) else {
                                continue;
                            };
                            if visited.contains(&link.target_entity) || chosen.contains_key(&t_idx)
                            {
                                continue;
                            }
                            let Some(values) = nd.fk_values.get(&link.target_entity) else {
                                continue;
                            };
                            if values.is_empty() {
                                continue;
                            }
                            let Some(pk_field) = entity_metas[t_idx].pk_dim_refs.first().cloned()
                            else {
                                continue;
                            };
                            chosen.insert(
                                t_idx,
                                vec![agentic_semantic::config::SemanticFilter {
                                    field: pk_field,
                                    filter_type: agentic_semantic::config::SemanticFilterType::In(
                                        agentic_semantic::config::ArrayFilter {
                                            values: values.clone(),
                                        },
                                    ),
                                }],
                            );
                        }
                    }

                    if chosen.is_empty() {
                        break;
                    }

                    // ── Build each node's query. Expandable nodes run ONE expansion
                    // query (PK + outbound-FK + label columns → matched, PK rows, FK
                    // values, and sample from a single scan). Terminal nodes run a
                    // cheap scalar count + a limited sample instead.
                    struct NodeWork<'a> {
                        meta: &'a EntityMeta,
                        expandable: bool,
                        plan: WmExpansionPlan,
                        /// Expansion query (expandable) or `__oxy_row_count` (leaf).
                        primary_cfg: SemanticQueryConfig,
                        /// Only for terminal nodes: a separate limited sample query.
                        sample_cfg: Option<SemanticQueryConfig>,
                    }
                    // Entities discovered THIS level — like `visited`, they are
                    // already accounted for, so a node whose only neighbours are in
                    // this set (or visited) has nothing new to reach and takes the
                    // cheap scalar-count path instead of the all-rows expansion.
                    let chosen_names: std::collections::HashSet<&str> = chosen
                        .keys()
                        .map(|&i| entity_metas[i].entity_name.as_str())
                        .collect();
                    let has_new_neighbor = |meta: &EntityMeta| -> bool {
                        let unseen = |n: &str| !visited.contains(n) && !chosen_names.contains(n);
                        meta.links.iter().any(|l| unseen(&l.target_entity))
                            || inbound.get(meta.entity_name.as_str()).is_some_and(|ch| {
                                ch.iter()
                                    .any(|&(ci, _)| unseen(&entity_metas[ci].entity_name))
                            })
                    };
                    let node_works: Vec<NodeWork<'_>> = chosen
                        .into_iter()
                        .map(|(idx, filters)| {
                            let meta = &entity_metas[idx];
                            // Only run the full (all-rows) expansion when there is a
                            // genuinely new neighbour to reach from this node; else a
                            // scalar count + limited sample suffices.
                            let expandable = has_new_neighbor(meta);
                            let plan = wm_expansion_plan(meta);
                            let (primary_cfg, sample_cfg) = if expandable {
                                (
                                    SemanticQueryConfig {
                                        topic: None,
                                        dimensions: plan.dims.clone(),
                                        measures: vec![],
                                        time_dimensions: vec![],
                                        filters,
                                        orders: vec![],
                                        limit: None,
                                        offset: None,
                                    },
                                    None,
                                )
                            } else {
                                (
                                    SemanticQueryConfig {
                                        topic: None,
                                        dimensions: vec![],
                                        measures: vec![format!(
                                            "{}.__oxy_row_count",
                                            meta.view_name
                                        )],
                                        time_dimensions: vec![],
                                        filters: filters.clone(),
                                        orders: vec![],
                                        limit: None,
                                        offset: None,
                                    },
                                    (!meta.sample_dims.is_empty()).then(|| SemanticQueryConfig {
                                        topic: None,
                                        dimensions: meta.sample_dims.clone(),
                                        measures: vec![],
                                        time_dimensions: vec![],
                                        filters,
                                        // Order ascending on the first sample dim so preview
                                        // chips are stable across reloads and align with the
                                        // ascending Sample Browser (`get_world_model_filter_instances`).
                                        orders: vec![SemanticOrder {
                                            field: meta.sample_dims[0].clone(),
                                            direction: "asc".to_string(),
                                        }],
                                        limit: Some(3),
                                        offset: None,
                                    }),
                                )
                            };
                            NodeWork {
                                meta,
                                expandable,
                                plan,
                                primary_cfg,
                                sample_cfg,
                            }
                        })
                        .collect();

                    if node_works.is_empty() {
                        break;
                    }

                    // Compile the whole level in one batch (primary first, then the
                    // optional leaf sample), under a single engine lock.
                    let all_cfgs: Vec<SemanticQueryConfig> = node_works
                        .iter()
                        .flat_map(|w| {
                            let mut v = vec![w.primary_cfg.clone()];
                            if let Some(ref s) = w.sample_cfg {
                                v.push(s.clone());
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
                                node_works
                                    .iter()
                                    .map(|w| 1 + w.sample_cfg.is_some() as usize)
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
                    struct NodeResult {
                        entity_name: String,
                        matched: u64,
                        /// Present for expandable nodes that matched → next hop.
                        neighbor: Option<NeighborData>,
                        sample: Vec<String>,
                        sample_keys: Vec<String>,
                    }
                    let exec_futures: Vec<_> = node_works
                        .into_iter()
                        .map(|w| {
                            // Pull SQL in push order: primary, then leaf sample (if any).
                            let primary_sql = sql_iter.next().flatten();
                            let sample_sql = w
                                .sample_cfg
                                .as_ref()
                                .and_then(|_| sql_iter.next().flatten());
                            let datasource = w.meta.datasource.clone();
                            let entity_name = w.meta.entity_name.clone();
                            let pk_count = w.meta.pk_count;
                            let has_label_dim = w.meta.has_label_dim;
                            let expandable = w.expandable;
                            let plan = w.plan;
                            let wm = wm_b.clone();
                            let role_c = role_b.clone();
                            let run_exp = &run_expansion;
                            async move {
                                if expandable {
                                    // One query does count + PK rows + FK values + sample.
                                    let res = run_exp(datasource, primary_sql, plan).await;
                                    let neighbor = (res.matched > 0).then(|| NeighborData {
                                        pk_rows: res.pk_rows,
                                        fk_values: res.fk_values.into_iter().collect(),
                                    });
                                    NodeResult {
                                        entity_name,
                                        matched: res.matched,
                                        neighbor,
                                        sample: res.sample,
                                        sample_keys: res.sample_keys,
                                    }
                                } else {
                                    // Terminal node: scalar count + limited sample,
                                    // fired concurrently on one connector.
                                    let connector =
                                        build_connector(&wm, user_id, role_c, &datasource)
                                            .await
                                            .ok();
                                    let (matched, (sample, sample_keys)) = match connector.as_ref()
                                    {
                                        Some(c) => tokio::join!(
                                            async {
                                                match primary_sql {
                                                    Some(sql) => run_with_connector(c, &sql, &wm)
                                                        .await
                                                        .into_iter()
                                                        .next()
                                                        .and_then(|r| r.into_iter().next())
                                                        .and_then(|v: String| v.parse::<u64>().ok())
                                                        .unwrap_or(0),
                                                    None => 0,
                                                }
                                            },
                                            async {
                                                match sample_sql {
                                                    Some(sql) => run_with_connector(c, &sql, &wm)
                                                        .await
                                                        .into_iter()
                                                        .map(|r| {
                                                            sample_row_to_display_key(
                                                                &r,
                                                                pk_count,
                                                                has_label_dim,
                                                            )
                                                        })
                                                        .unzip(),
                                                    None => (vec![], vec![]),
                                                }
                                            },
                                        ),
                                        None => (0, (vec![], vec![])),
                                    };
                                    NodeResult {
                                        entity_name,
                                        matched,
                                        neighbor: None,
                                        sample,
                                        sample_keys,
                                    }
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
                    // Mark every discovered entity visited (count-once, cycle guard)
                    // and seed the next frontier from expandable nodes that matched.
                    let mut next_frontier: Vec<String> = Vec::new();
                    let mut client_gone = false;
                    for r in results {
                        visited.insert(r.entity_name.clone());
                        let sent = tx_b
                            .send(WmFilterCountEvent {
                                entity_name: r.entity_name.clone(),
                                matched: Some(r.matched),
                                total: None,
                                sample: r.sample,
                                sample_keys: r.sample_keys,
                                done: false,
                            })
                            .await;
                        if sent.is_err() {
                            client_gone = true;
                            break;
                        }
                        if let Some(nd) = r.neighbor {
                            next_frontier.push(r.entity_name.clone());
                            neighbor_data.insert(r.entity_name, nd);
                        }
                    }
                    if client_gone {
                        // Receiver dropped — don't schedule further BFS levels. The
                        // level that just finished already ran to completion (its
                        // queries were in flight before we could detect this), but
                        // this stops the compounding growth described in
                        // `WmExpansionResult` from continuing hop after hop once
                        // nobody is listening.
                        tracing::debug!(
                            depth,
                            "filter-counts BFS: receiver dropped, stopping early"
                        );
                        break;
                    }
                    frontier = next_frontier;
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
            sample: vec![],
            sample_keys: vec![],
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
        let make_cfg = |nodes: &[&WmBreakdownNode]| SemanticQueryConfig {
            topic: None,
            dimensions: vec![],
            measures: nodes
                .iter()
                .map(|n| format!("{}.{}", n.view, n.measure))
                .collect(),
            time_dimensions: vec![],
            filters: filters.clone(),
            orders: vec![],
            limit: Some(1),
            offset: None,
        };
        // A composite node is a cross-view roll-up; bundling more than one into a
        // single SELECT co-locates their independent one-to-many joins into a
        // shared CTE and trips airlayer's fan-out guard, failing the *whole* group
        // (and any additive sibling in it) — the same batching hazard the
        // instance-detail own-measure queries avoid. Give each composite its own
        // query; keep plain single-view nodes batched into one round-trip.
        let simple: Vec<&WmBreakdownNode> = group_nodes
            .iter()
            .copied()
            .filter(|n| !n.is_composite)
            .collect();
        if !simple.is_empty() {
            groups.push((
                view_name.clone(),
                simple.iter().map(|n| n.id.clone()).collect(),
                make_cfg(&simple),
            ));
        }
        for n in group_nodes.iter().copied().filter(|n| n.is_composite) {
            groups.push((view_name.clone(), vec![n.id.clone()], make_cfg(&[n])));
        }
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
    // Inbound neighbours of the selected instance — every entity that references
    // it via a FK. This is the union of two link kinds, both queried identically
    // (filter the child's FK-to-q.entity by the seed key):
    //   • Parent-spine children (their `parent:` is q.entity) — measure promotions
    //     like `order_item → order`; shown first.
    //   • Cross-link children (they declare q.entity as a Foreign entity without
    //     naming it parent) — e.g. `order → retail_store` ("orders at this store"),
    //     the case the parent tree missed.
    let mut inbound_children: Vec<(String, LinkKind)> = Vec::new();
    for v in &layer.views {
        let Some(primary) = v
            .entities
            .iter()
            .find(|e| e.entity_type == EntityType::Primary)
        else {
            continue;
        };
        if primary.name == q.entity {
            continue;
        }
        let parent = promotions.parent_of(&primary.name);
        for link in build_entity_links(&layer.views, v, parent) {
            if link.target_entity == q.entity {
                inbound_children.push((primary.name.clone(), link.kind));
            }
        }
    }
    // Parent-spine promotions before cross-link references (stable within groups).
    inbound_children.sort_by_key(|(_, kind)| match kind {
        LinkKind::Parent => 0,
        LinkKind::CrossLink => 1,
    });

    let child_cfgs: Vec<ChildCfg> = inbound_children
        .iter()
        .filter_map(|(child_entity, _kind)| {
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

    // 4. Own measures. `measure_meta` (view order) seeds the frontend skeleton
    //    rows via MeasureNames; the frontend fills them by name, so the value
    //    queries below can group measures however is convenient.
    #[derive(Clone)]
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
    let make_meta = |m: &airlayer::Measure| MeasureMeta {
        name: m.name.clone(),
        measure_type: format!("{:?}", m.measure_type).to_lowercase(),
        label: meas_allow
            .as_ref()
            .and_then(|a| a.get(m.name.as_str()).cloned().flatten()),
    };
    let measure_meta: Vec<MeasureMeta> = own_measures.iter().map(|m| make_meta(m)).collect();

    // Value queries. A `custom` measure is a cross-view composite (its expr rolls
    // up measures from other views); bundling several into one SELECT drags each
    // one's independent one-to-many join into a shared CTE, tripping airlayer's
    // fan-out / additive-vs-non-additive guard and failing the *whole* batch. So
    // each composite gets its own query (airlayer isolates a single composite's
    // terms into per-view CTEs correctly), while plain single-view measures stay
    // batched into one round-trip.
    struct OwnGroup {
        measures: Vec<MeasureMeta>,
        cfg: SemanticQueryConfig,
    }
    let own_cfg = |measures: &[&airlayer::Measure]| SemanticQueryConfig {
        topic: None,
        dimensions: vec![],
        measures: measures
            .iter()
            .map(|m| format!("{}.{}", view.name, m.name))
            .collect(),
        time_dimensions: vec![],
        filters: pk_filters.clone(),
        orders: vec![],
        limit: Some(1),
        offset: None,
    };
    let mut own_groups: Vec<OwnGroup> = Vec::new();
    let simple: Vec<&airlayer::Measure> = own_measures
        .iter()
        .copied()
        .filter(|m| m.measure_type != MeasureType::Custom)
        .collect();
    if !simple.is_empty() {
        own_groups.push(OwnGroup {
            measures: simple.iter().map(|m| make_meta(m)).collect(),
            cfg: own_cfg(&simple),
        });
    }
    for m in own_measures
        .iter()
        .copied()
        .filter(|m| m.measure_type == MeasureType::Custom)
    {
        own_groups.push(OwnGroup {
            measures: vec![make_meta(m)],
            cfg: own_cfg(&[m]),
        });
    }

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
        if let Some(source_view) = layer.views.iter().find(|v| v.name == im.source_view)
            && let Some(sm) = source_view
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
    let own_cfgs: Vec<SemanticQueryConfig> = own_groups.iter().map(|g| g.cfg.clone()).collect();

    // --- Phase 1: compile ALL SQL configs (except parent which needs FK from attrs) ---
    let layer_clone = (*layer).clone();
    let dbs_clone = databases.clone();
    type SqlOpt = Option<String>;
    let phase1: Option<(SqlOpt, Vec<SqlOpt>, Vec<SqlOpt>, Vec<SqlOpt>, Vec<SqlOpt>)> =
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
                child_sample_cfgs.iter().map(&c).collect(),
                child_count_cfgs.iter().map(&c).collect(),
                own_cfgs.iter().map(&c).collect(),
                induced_cfgs.iter().map(c).collect(),
            ))
        })
        .await
        .ok()
        .and_then(|r| r.ok());
    let (attrs_sql, child_sample_sqls, child_count_sqls, own_group_sqls, induced_sqls) =
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
                                    sample_row_to_display_key(&r, cc.pk_count, cc.has_label_dim)
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
                // Tag distinguishes an own-measure group from an induced group so the
                // right column→measure mapping is applied on completion.
                enum GroupTag {
                    Own(usize),
                    Induced(usize),
                }
                type Rows = Vec<Vec<String>>;
                let mut futs: FuturesUnordered<
                    std::pin::Pin<Box<dyn std::future::Future<Output = (GroupTag, Rows)> + Send>>,
                > = FuturesUnordered::new();

                for (idx, sql_opt) in own_group_sqls.into_iter().enumerate() {
                    let c = connector_c.clone();
                    let wm = wm_c.clone();
                    futs.push(Box::pin(async move {
                        let rows = match sql_opt {
                            Some(ref sql) => run_with_connector(&c, sql, &wm).await,
                            None => vec![],
                        };
                        (GroupTag::Own(idx), rows)
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
                        (GroupTag::Induced(idx), rows)
                    }));
                }

                while let Some((tag, rows)) = futs.next().await {
                    let computed_measures: Vec<WmComputedMeasure> = match tag {
                        GroupTag::Own(idx) => {
                            let group = &own_groups[idx];
                            let own_row = rows.into_iter().next().unwrap_or_default();
                            group
                                .measures
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
                        GroupTag::Induced(idx) => {
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

    fn bnode(measure: &str, composite: bool) -> WmBreakdownNode {
        WmBreakdownNode {
            id: format!("orders.{measure}"),
            view: "orders".into(),
            measure: measure.into(),
            label: measure.into(),
            measure_type: "number".into(),
            is_composite: composite,
            is_root: false,
            expr: None,
        }
    }

    // Regression: several cross-view composites of the same view must NOT be
    // bundled into one query (that trips airlayer's fan-out guard and fails the
    // whole group). Each composite gets its own group; plain nodes stay batched.
    #[test]
    fn breakdown_isolates_each_composite_into_its_own_group() {
        let orders: airlayer::View = serde_json::from_value(serde_json::json!({
            "name": "orders",
            "table": "orders",
            "entities": [{"name": "order", "type": "primary", "key": "order_id"}],
            "dimensions": [{"name": "order_id", "type": "number", "expr": "id"}],
        }))
        .expect("valid view");
        let layer = airlayer::SemanticLayer::new(vec![orders], None);
        let nodes = vec![
            bnode("net_revenue", true),
            bnode("total_order_value", true),
            bnode("total_shipping_costs", true),
            bnode("total_tax_collected", false),
        ];

        let plan = breakdown_value_plan(
            &layer,
            &nodes,
            "order",
            &["1".to_string()],
            &["order_id".to_string()],
            "orders",
        );

        assert!(plan.unvalued.is_empty());
        // 3 single-composite groups + 1 batched group for the plain node.
        assert_eq!(plan.groups.len(), 4);
        // No group bundles more than one measure alongside a composite.
        for (_, node_ids, cfg) in &plan.groups {
            let has_composite = node_ids
                .iter()
                .any(|id| nodes.iter().any(|n| &n.id == id && n.is_composite));
            if has_composite {
                assert_eq!(cfg.measures.len(), 1, "composite group must be isolated");
            }
        }
        // The single plain node is batched on its own here (only one exists).
        let plain = plan
            .groups
            .iter()
            .find(|(_, ids, _)| ids.iter().any(|id| id == "orders.total_tax_collected"))
            .expect("plain node group present");
        assert_eq!(plain.1, vec!["orders.total_tax_collected".to_string()]);
    }
}
