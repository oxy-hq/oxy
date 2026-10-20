//! Place in the semantic model: resolving a warehouse key to a location, and
//! scoping a semantic query to a caller's reach.
//!
//! A view's primary entity may carry `binding: { registry: locations, system:
//! toast }` (parsed by `oxy-airlayer-compat`, read through `bound_views`).
//! With it, the key the warehouse carries — a Toast GUID — is what `toast`
//! calls one of the org's locations, and `location_external_ids` says which.
//! Two directions: key → place, for naming an instance; places → keys, for
//! turning a caller's reach into a WHERE. `internal-docs/operating-graph.md`
//! §3.6.

use agentic_semantic::config::{
    ArrayFilter, SemanticFilter, SemanticFilterType, SemanticQueryConfig,
};
use entity::{location_external_ids as ext, locations};
use oxy_airlayer_compat::{BoundView, SemanticLayer, bound_views};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::reach::Reach;

/// What an instance learns about its place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocationBrief {
    pub id: Uuid,
    pub name: String,
    pub kind: Option<String>,
    pub parent_id: Option<Uuid>,
}

/// key → place, for every key `system` knows in this org. Keys that map to
/// nothing are simply absent: an unmapped store is a fact, not an error.
pub async fn locations_by_external_ids(
    db: &DatabaseConnection,
    org_id: Uuid,
    system: &str,
    keys: &[String],
) -> Result<HashMap<String, LocationBrief>, DbErr> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = ext::Entity::find()
        .filter(ext::Column::OrgId.eq(org_id))
        .filter(ext::Column::System.eq(system))
        .filter(ext::Column::ExternalId.is_in(keys.to_vec()))
        .all(db)
        .await?;
    if rows.is_empty() {
        return Ok(HashMap::new());
    }
    let ids: Vec<Uuid> = rows.iter().map(|r| r.location_id).collect();
    let places: HashMap<Uuid, locations::Model> = locations::Entity::find()
        .filter(locations::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|l| (l.id, l))
        .collect();
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let l = places.get(&r.location_id)?;
            Some((
                r.external_id,
                LocationBrief {
                    id: l.id,
                    name: l.name.clone(),
                    kind: l.kind.clone(),
                    parent_id: l.parent_id,
                },
            ))
        })
        .collect())
}

/// places → keys: what `system` calls each of these locations. A location
/// with no id in that system contributes nothing — it cannot be named in the
/// warehouse, so it cannot be reached there either.
pub async fn external_ids_for_locations(
    db: &DatabaseConnection,
    org_id: Uuid,
    system: &str,
    location_ids: &[Uuid],
) -> Result<Vec<String>, DbErr> {
    if location_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids: Vec<String> = ext::Entity::find()
        .filter(ext::Column::OrgId.eq(org_id))
        .filter(ext::Column::System.eq(system))
        .filter(ext::Column::LocationId.is_in(location_ids.to_vec()))
        .all(db)
        .await?
        .into_iter()
        .map(|r| r.external_id)
        .collect();
    ids.sort_unstable();
    Ok(ids)
}

/// The views a query touches, by the `view.member` prefix of every member it
/// names. The topic is not consulted: a member is qualified whatever the
/// topic, and a topic that joins views the query never names does not need
/// scoping on them.
pub fn referenced_views(query: &SemanticQueryConfig) -> HashSet<String> {
    query
        .measures
        .iter()
        .chain(query.dimensions.iter())
        .chain(query.time_dimensions.iter().map(|t| &t.dimension))
        .chain(query.filters.iter().map(|f| &f.field))
        .filter_map(|m| m.split_once('.').map(|(view, _)| view.to_string()))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum ScopeError {
    #[error(
        "scope `reach` needs a view whose primary entity is bound to the locations registry, \
         and this query names none"
    )]
    NoBoundView,
    #[error("database error: {0}")]
    Db(#[from] DbErr),
}

/// A value no warehouse key equals. A scoped caller whose places have no id in
/// the bound system must match nothing, and an empty `IN ()` is not SQL in
/// every dialect — so the list is never empty, it names nothing instead.
pub const NO_REACH_SENTINEL: &str = "__oxy_reach_nothing__";

/// The filters that pin `query` to `reach`, given the bound views it names
/// and, per system, the keys the caller's places carry. Pure, so the decision
/// table is testable without a layer or a database.
pub fn scope_filters(
    bound: &[BoundView],
    referenced: &HashSet<String>,
    keys_by_system: &HashMap<String, Vec<String>>,
) -> Result<Vec<SemanticFilter>, ScopeError> {
    let touched: Vec<&BoundView> = bound
        .iter()
        .filter(|b| referenced.contains(&b.view))
        .collect();
    if touched.is_empty() {
        return Err(ScopeError::NoBoundView);
    }
    Ok(touched
        .into_iter()
        .map(|b| {
            let keys = keys_by_system
                .get(&b.binding.system)
                .filter(|k| !k.is_empty())
                .cloned()
                .unwrap_or_else(|| vec![NO_REACH_SENTINEL.to_string()]);
            SemanticFilter {
                field: format!("{}.{}", b.view, b.key),
                filter_type: SemanticFilterType::In(ArrayFilter {
                    values: keys.into_iter().map(serde_json::Value::String).collect(),
                }),
            }
        })
        .collect())
}

/// Pin `query` to the caller's reach. A caller who reaches everywhere leaves
/// the query alone; anyone else gets one `in` filter per bound view the query
/// names, over the keys their places carry in that view's system. Fails —
/// rather than silently answering everything — when the query names no bound
/// view: `scope: reach` is a promise the caller is relying on.
pub async fn apply_reach_scope(
    db: &DatabaseConnection,
    org_id: Uuid,
    layer: &SemanticLayer,
    reach: &Reach,
    query: &mut SemanticQueryConfig,
) -> Result<(), ScopeError> {
    let bound = bound_views(layer);
    let referenced = referenced_views(query);
    if reach.everywhere {
        // Still refuse an unscopable query: a caller asking for `reach` on a
        // model with nothing bound would otherwise learn nothing is wrong
        // until the day somebody scoped is asking.
        if !bound.iter().any(|b| referenced.contains(&b.view)) {
            return Err(ScopeError::NoBoundView);
        }
        return Ok(());
    }
    let systems: HashSet<String> = bound
        .iter()
        .filter(|b| referenced.contains(&b.view))
        .map(|b| b.binding.system.clone())
        .collect();
    let mut keys_by_system = HashMap::new();
    for system in systems {
        let keys = external_ids_for_locations(db, org_id, &system, &reach.locations).await?;
        keys_by_system.insert(system, keys);
    }
    let filters = scope_filters(&bound, &referenced, &keys_by_system)?;
    query.filters.extend(filters);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxy_airlayer_compat::EntityBinding;

    fn bound(view: &str, key: &str, system: &str) -> BoundView {
        BoundView {
            view: view.into(),
            entity: "store".into(),
            key: key.into(),
            binding: EntityBinding {
                registry: "locations".into(),
                system: system.into(),
            },
        }
    }

    fn query(members: &[&str]) -> SemanticQueryConfig {
        serde_json::from_value(serde_json::json!({
            "topic": "sales",
            "measures": members,
        }))
        .unwrap()
    }

    #[test]
    fn the_views_a_query_names_are_its_member_prefixes() {
        let q: SemanticQueryConfig = serde_json::from_value(serde_json::json!({
            "topic": "sales",
            "measures": ["sales.total"],
            "dimensions": ["stores.region"],
            "time_dimensions": [{ "dimension": "sales.day", "granularity": "day" }],
            "filters": [{ "field": "labor.hours", "op": "gt", "value": 1 }],
        }))
        .unwrap();
        let mut views: Vec<String> = referenced_views(&q).into_iter().collect();
        views.sort();
        assert_eq!(views, vec!["labor", "sales", "stores"]);
    }

    #[test]
    fn a_scoped_caller_gets_one_in_filter_per_bound_view_they_name() {
        let bound = vec![
            bound("sales", "restaurant_id", "toast"),
            bound("labor", "site", "payroll"),
        ];
        let referenced: HashSet<String> = ["sales", "labor", "weather"]
            .into_iter()
            .map(String::from)
            .collect();
        let mut keys = HashMap::new();
        keys.insert(
            "toast".to_string(),
            vec!["g1".to_string(), "g2".to_string()],
        );
        // payroll: the caller's places carry no payroll id.
        let filters = scope_filters(&bound, &referenced, &keys).unwrap();
        let by_field: HashMap<String, Vec<serde_json::Value>> = filters
            .into_iter()
            .map(|f| match f.filter_type {
                SemanticFilterType::In(a) => (f.field, a.values),
                other => panic!("expected an `in` filter, got {other:?}"),
            })
            .collect();
        assert_eq!(by_field["sales.restaurant_id"], vec!["g1", "g2"]);
        // Nothing mapped → a list that names nothing, never an empty IN and
        // never no filter.
        assert_eq!(by_field["labor.site"], vec![NO_REACH_SENTINEL]);
        assert_eq!(by_field.len(), 2);
    }

    #[test]
    fn a_query_naming_no_bound_view_cannot_be_scoped() {
        let bound = vec![bound("sales", "restaurant_id", "toast")];
        let referenced: HashSet<String> = ["weather"].into_iter().map(String::from).collect();
        assert!(matches!(
            scope_filters(&bound, &referenced, &HashMap::new()),
            Err(ScopeError::NoBoundView)
        ));
        assert!(referenced_views(&query(&["weather.temp"])).contains("weather"));
    }
}
