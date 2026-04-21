//! View/measure/dimension lookup helpers + name qualification + fuzzy normalizer.

use super::SemanticCatalog;

/// Normalize a name for fuzzy comparison: lowercase, strip underscores/hyphens,
/// collapse whitespace.  `"Max Heart Rate"` and `"max_heart_rate"` both become
/// `"maxheartrate"`.
pub(super) fn normalize_for_fuzzy(s: &str) -> String {
    s.to_lowercase()
        .replace(['_', '-'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("")
}

impl SemanticCatalog {
    /// Check whether a view named `table` has a dimension or measure named
    /// `column` (case-insensitive on both).
    pub(super) fn field_exists(&self, table: &str, column: &str) -> bool {
        let col_lc = column.to_lowercase();
        for view in self.engine.views() {
            if view.name.eq_ignore_ascii_case(table) {
                if view
                    .dimensions
                    .iter()
                    .any(|d| d.name.to_lowercase() == col_lc)
                {
                    return true;
                }
                if view
                    .measures_list()
                    .iter()
                    .any(|m| m.name.to_lowercase() == col_lc)
                {
                    return true;
                }
                // Also check view-qualified name.
                use crate::catalog::Catalog;
                if self
                    .get_metric_definition(&format!("{}.{}", view.name, column))
                    .is_some()
                {
                    return true;
                }
                return false;
            }
        }
        // Check if `table` is the underlying table name of a view.
        use crate::catalog::Catalog;
        if let Some(def) = self.get_metric_definition(column)
            && def.table.eq_ignore_ascii_case(table)
        {
            return true;
        }
        false
    }

    /// Find the view and measure definition for `metric`.
    ///
    /// Accepts both bare names (`"revenue"`) and view-qualified names
    /// (`"orders_view.revenue"`).
    pub(super) fn find_measure<'a>(
        &'a self,
        metric: &str,
    ) -> Option<(&'a airlayer::View, &'a airlayer::Measure)> {
        self.engine.views().iter().find_map(|v| {
            v.measures_list()
                .iter()
                .find(|m| m.name == metric || format!("{}.{}", v.name, m.name) == metric)
                .map(|m| (v, m))
        })
    }

    /// Find the view and dimension definition for `dim`.
    pub(super) fn find_dimension<'a>(
        &'a self,
        dim: &str,
    ) -> Option<(&'a airlayer::View, &'a airlayer::Dimension)> {
        self.engine.views().iter().find_map(|v| {
            v.dimensions
                .iter()
                .find(|d| d.name == dim || format!("{}.{}", v.name, d.name) == dim)
                .map(|d| (v, d))
        })
    }

    /// Return all views reachable from `start_view` via entity joins.
    ///
    /// A view `B` is joinable from `A` when `A` has a `primary` entity whose
    /// key matches a `foreign` entity key in `B`.
    pub(super) fn reachable_views<'a>(
        &'a self,
        start: &'a airlayer::View,
    ) -> Vec<&'a airlayer::View> {
        let mut reachable = vec![start];
        let primary_keys: Vec<String> = start
            .entities
            .iter()
            .filter(|e| e.entity_type == airlayer::schema::models::EntityType::Primary)
            .flat_map(|e| e.get_keys())
            .collect();

        for view in self.engine.views() {
            if view.name == start.name {
                continue;
            }
            let joinable = view.entities.iter().any(|e| {
                e.entity_type == airlayer::schema::models::EntityType::Foreign
                    && e.get_keys().iter().any(|k| primary_keys.contains(k))
            });
            if joinable && !reachable.iter().any(|r| r.name == view.name) {
                reachable.push(view);
            }
        }
        reachable
    }

    /// Qualify bare metric/dimension names to `ViewName.field` format.
    ///
    /// When `preferred_views` is non-empty, fields that exist in a preferred
    /// view are resolved there first.  This is critical for dimensions like
    /// `"date"` that appear in every view — without a hint the resolver would
    /// pick whichever view comes first in the list, which may differ from the
    /// metrics' view and cause an unjoinable cross-view query.
    ///
    /// Resolution tiers (tried in order, preferred views first at each tier):
    /// 1. Exact name match (case-sensitive)
    /// 2. Case-insensitive match
    /// 3. Fuzzy match (Jaro-Winkler ≥ 0.8)
    ///
    /// Returns `None` if any name cannot be resolved.
    pub(super) fn qualify_names(
        &self,
        names: &[String],
        is_metric: bool,
        preferred_views: &[String],
    ) -> Option<Vec<String>> {
        names
            .iter()
            .map(|name| {
                // If already dot-qualified AND matches a real view.field, accept as-is.
                if name.contains('.') {
                    let is_known = self.engine.views().iter().any(|v| {
                        let prefix = format!("{}.", v.name);
                        if let Some(field) = name.strip_prefix(&prefix) {
                            if is_metric {
                                v.measures_list().iter().any(|m| m.name == field)
                            } else {
                                v.dimensions.iter().any(|d| d.name == field)
                            }
                        } else {
                            false
                        }
                    });
                    if is_known {
                        return Some(name.clone());
                    }
                }

                // Strip table prefix if present (LLM may qualify with raw table
                // name, e.g. "cardio_4_4.Max Heart Rate").
                let bare_name = name.find('.').map(|pos| &name[pos + 1..]).unwrap_or(name);

                // Build search order: preferred views first, then the rest.
                let all_views = self.engine.views();
                let ordered: Vec<&airlayer::View> = preferred_views
                    .iter()
                    .filter_map(|pv| all_views.iter().find(|v| &v.name == pv))
                    .chain(
                        all_views
                            .iter()
                            .filter(|v| !preferred_views.contains(&v.name)),
                    )
                    .collect();

                // Tier 1: exact match
                for view in &ordered {
                    if is_metric {
                        if view.measures_list().iter().any(|m| m.name == bare_name) {
                            return Some(format!("{}.{}", view.name, bare_name));
                        }
                    } else if view.dimensions.iter().any(|d| d.name == bare_name) {
                        return Some(format!("{}.{}", view.name, bare_name));
                    }
                }
                // Tier 2: case-insensitive match
                let lower = bare_name.to_lowercase();
                for view in &ordered {
                    if is_metric {
                        if let Some(m) = view
                            .measures_list()
                            .iter()
                            .find(|m| m.name.to_lowercase() == lower)
                        {
                            return Some(format!("{}.{}", view.name, m.name));
                        }
                    } else if let Some(d) = view
                        .dimensions
                        .iter()
                        .find(|d| d.name.to_lowercase() == lower)
                    {
                        return Some(format!("{}.{}", view.name, d.name));
                    }
                }
                // Tier 3: fuzzy match
                let normalized = normalize_for_fuzzy(bare_name);
                let mut best: Option<(f64, String, String)> = None;
                for view in &ordered {
                    if is_metric {
                        for m in view.measures_list() {
                            let score =
                                strsim::jaro_winkler(&normalized, &normalize_for_fuzzy(&m.name));
                            if score >= 0.8 && best.as_ref().is_none_or(|b| score > b.0) {
                                best = Some((score, view.name.clone(), m.name.clone()));
                            }
                        }
                    } else {
                        for d in &view.dimensions {
                            let score =
                                strsim::jaro_winkler(&normalized, &normalize_for_fuzzy(&d.name));
                            if score >= 0.8 && best.as_ref().is_none_or(|b| score > b.0) {
                                best = Some((score, view.name.clone(), d.name.clone()));
                            }
                        }
                    }
                }
                best.map(|(_, view, field)| format!("{view}.{field}"))
            })
            .collect()
    }
}
