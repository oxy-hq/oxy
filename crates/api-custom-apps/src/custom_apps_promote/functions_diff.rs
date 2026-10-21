//! Functions added, removed and changed between what an environment serves
//! and the build it would move to (environments design §5.3, §5.5a ruling 8).
//! *Changed* is a different manifest entry, or — the entries equal — bundled
//! JS whose bytes differ.

use std::collections::BTreeMap;

use entity::{app_builds, app_functions};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Serialize;

use super::error::PromoteError;
use crate::custom_apps_build_store as store;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionChanges {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
}

/// One build's functions: name → (manifest entry, artifact key).
type Functions = BTreeMap<String, (Option<serde_json::Value>, String)>;

/// `target` against `current` (`None`: the environment serves nothing, so
/// every function is added). The second half is a warning for each function
/// whose bytes could not be read — counted as changed, never as the same.
pub async fn diff(
    db: &DatabaseConnection,
    target: &app_builds::Model,
    current: Option<&app_builds::Model>,
) -> Result<(FunctionChanges, Vec<String>), PromoteError> {
    let new = functions_of(db, target).await?;
    let old = match current {
        Some(current) => functions_of(db, current).await?,
        None => Functions::new(),
    };
    let (mut changes, same_entries) = classify(&new, &old);
    let mut warnings = Vec::new();
    if let Some(current) = current {
        for name in same_entries {
            match same_bytes(target, current, &new[&name].1, &old[&name].1).await {
                Ok(true) => {}
                Ok(false) => changes.changed.push(name),
                Err(e) => {
                    warnings.push(format!("could not compare function {name}'s code ({e})"));
                    changes.changed.push(name);
                }
            }
        }
    }
    changes.changed.sort();
    Ok((changes, warnings))
}

/// Added, removed, changed by manifest entry — and the names whose entries
/// are equal, whose bytes the caller compares.
fn classify(new: &Functions, old: &Functions) -> (FunctionChanges, Vec<String>) {
    let mut changes = FunctionChanges::default();
    let mut same_entries = Vec::new();
    for (name, (entry, _)) in new {
        match old.get(name) {
            None => changes.added.push(name.clone()),
            Some((was, _)) if was != entry => changes.changed.push(name.clone()),
            Some(_) => same_entries.push(name.clone()),
        }
    }
    changes.removed = old
        .keys()
        .filter(|name| !new.contains_key(*name))
        .cloned()
        .collect();
    (changes, same_entries)
}

async fn functions_of(
    db: &DatabaseConnection,
    build: &app_builds::Model,
) -> Result<Functions, PromoteError> {
    Ok(app_functions::Entity::find()
        .filter(app_functions::Column::BuildId.eq(build.id))
        .all(db)
        .await
        .map_err(|e| PromoteError::internal("read the build's functions", e))?
        .into_iter()
        .map(|f| (f.name, (f.manifest_json, f.artifact_key)))
        .collect())
}

/// Whether the two artifacts hold the same bytes.
async fn same_bytes(
    a: &app_builds::Model,
    b: &app_builds::Model,
    key_a: &str,
    key_b: &str,
) -> Result<bool, store::BuildStoreError> {
    let read = |build: &app_builds::Model, key: &str| {
        let prefix = store::build_prefix(build.app_id, &build.build_id);
        let rel = key.strip_prefix(&prefix).unwrap_or(key).to_string();
        let (app, label) = (build.app_id, build.build_id.clone());
        async move { store::get_object(app, &label, &rel).await }
    };
    let (left, right) = (read(a, key_a).await?, read(b, key_b).await?);
    match (left, right) {
        (Some(left), Some(right)) => Ok(left == right),
        _ => Err(store::BuildStoreError::Io("an artifact is missing".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn functions(entries: &[(&str, serde_json::Value)]) -> Functions {
        entries
            .iter()
            .map(|(name, entry)| {
                (
                    name.to_string(),
                    (Some(entry.clone()), format!("functions/{name}.js")),
                )
            })
            .collect()
    }

    #[test]
    fn entries_sort_functions_into_added_removed_changed_and_compared() {
        let old = functions(&[
            ("kept", json!({ "route": true })),
            ("retimed", json!({ "schedule": "0 * * * *" })),
            ("dropped", json!({ "route": true })),
        ]);
        let new = functions(&[
            ("kept", json!({ "route": true })),
            ("retimed", json!({ "schedule": "0 0 * * *" })),
            ("fresh", json!({ "route": true })),
        ]);
        let (changes, same) = classify(&new, &old);
        assert_eq!(changes.added, vec!["fresh"]);
        assert_eq!(changes.removed, vec!["dropped"]);
        assert_eq!(changes.changed, vec!["retimed"]);
        assert_eq!(same, vec!["kept"], "equal entries are compared by bytes");
    }
}
