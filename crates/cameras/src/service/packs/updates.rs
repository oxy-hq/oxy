//! Starter-update detection and per-role merge.
//!
//! Oxy curates the starter library; operators fork it into a
//! workspace at the version published at fork time. When Oxy bumps
//! a starter version, [`pending_updates`] computes a per-pack +
//! per-role diff so the curator UI can render "Take Oxy's / Keep
//! mine" buttons. [`apply_update`] commits the operator's choices
//! and bumps the pack's `starter_version`.
//!
//! ## What we don't have
//!
//! We don't ship every historical starter body — only the current
//! published version. That means we can't do a true 3-way merge
//! ("who moved first?"). We do a 2-way diff: current pack body vs
//! current upstream body. The `locally_modified` flag (set
//! whenever the operator touches the body) is the closest signal
//! we have to "the operator has divergent intent here."
//!
//! For most pack updates (typo fixes, prompt-wording tweaks) a
//! 2-way diff is sufficient — operators just want to see what
//! changed and accept or reject. A future iteration could ship
//! historical starter bodies if 3-way merge becomes necessary,
//! but the cost (binary size, schema rev) doesn't justify it for
//! v1.

use super::body::{PackBody, RolePrompt};
use crate::entities::domain_packs;
use crate::service::{ServiceError, ServiceResult};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Sentinel key for the "should we take upstream's variables map"
/// decision in [`apply_update`]'s action map. Real role_keys can't
/// start with `$` (they're free text but the UI restricts them),
/// so this never collides.
pub const VARIABLES_ACTION_KEY: &str = "$variables";

/// One pending update for a single pack in the workspace.
#[derive(Debug, Serialize)]
pub struct PackStarterUpdate {
    pub pack_key: String,
    pub starter_key: String,
    pub current_version: String,
    pub available_version: String,
    pub locally_modified: bool,
    pub role_diffs: Vec<RoleDiff>,
    pub variables_differ: bool,
    pub current_variables: serde_json::Value,
    pub upstream_variables: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct RoleDiff {
    pub role_key: String,
    pub status: RoleDiffStatus,
    pub current: Option<RolePrompt>,
    pub upstream: Option<RolePrompt>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleDiffStatus {
    /// Role exists on both sides and the prompt + label + hint
    /// are identical. Operators don't need to do anything.
    Unchanged,
    /// Role exists on both sides but one or more fields differ.
    Differs,
    /// Role is in the new starter but not in the workspace's
    /// current body — operator added the pack before Oxy
    /// introduced this role.
    OnlyInUpstream,
    /// Role is in the workspace's current body but not in the new
    /// starter — operator removed it, or Oxy deprecated it.
    OnlyInCurrent,
}

/// Operator's per-key decision when applying a starter update.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAction {
    /// Replace the local value with upstream's. For a role: take
    /// upstream's prompt/label/hint. For variables (via
    /// [`VARIABLES_ACTION_KEY`]): replace the whole map.
    TakeUpstream,
    /// Keep the local value as-is. For a role only in upstream,
    /// this means "don't add it." For a role only in current,
    /// this means "keep my removal" — actually keep it because the
    /// operator might still want it. See default semantics below.
    KeepMine,
}

/// List every pack in the workspace that has an upstream update
/// available. A pack with no available bump simply omits from the
/// result; a workspace with no updates returns an empty list.
///
/// Internally per pack:
///   1. Look up the starter's current version. If unknown (operator
///      forked a starter Oxy has since renamed), skip.
///   2. If the workspace's `starter_version` already matches, skip.
///   3. Compute the role diff and variables diff against
///      [`super::starter::body_for`].
pub async fn pending_updates(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Vec<PackStarterUpdate>> {
    let rows = domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .all(db)
        .await?;

    let mut out = Vec::new();
    for row in rows {
        let Some(upstream_version) = super::starter::version_for(&row.starter_key) else {
            continue;
        };
        if upstream_version == row.starter_version {
            continue;
        }
        let Some(upstream_body) = super::starter::body_for(&row.starter_key) else {
            continue;
        };
        let current_body: PackBody = match serde_json::from_value(row.body.clone()) {
            Ok(b) => b,
            Err(_) => continue, // body is malformed — skip rather than crash list
        };

        out.push(diff_pack(
            row.pack_key,
            row.starter_key,
            row.starter_version,
            upstream_version.to_string(),
            row.locally_modified,
            current_body,
            upstream_body,
        ));
    }
    Ok(out)
}

fn diff_pack(
    pack_key: String,
    starter_key: String,
    current_version: String,
    available_version: String,
    locally_modified: bool,
    current: PackBody,
    upstream: PackBody,
) -> PackStarterUpdate {
    // Index roles by role_key for O(1) lookup. BTreeMap so the
    // diff output is deterministic — easier for the UI to render
    // a stable order, and easier for tests to assert against.
    let current_roles: BTreeMap<String, RolePrompt> = current
        .roles
        .into_iter()
        .map(|r| (r.role_key.clone(), r))
        .collect();
    let upstream_roles: BTreeMap<String, RolePrompt> = upstream
        .roles
        .into_iter()
        .map(|r| (r.role_key.clone(), r))
        .collect();

    let mut all_keys: Vec<String> = current_roles
        .keys()
        .chain(upstream_roles.keys())
        .cloned()
        .collect();
    all_keys.sort();
    all_keys.dedup();

    let role_diffs: Vec<RoleDiff> = all_keys
        .into_iter()
        .map(|key| {
            let cur = current_roles.get(&key).cloned();
            let ups = upstream_roles.get(&key).cloned();
            let status = match (&cur, &ups) {
                (Some(c), Some(u)) if c == u => RoleDiffStatus::Unchanged,
                (Some(_), Some(_)) => RoleDiffStatus::Differs,
                (Some(_), None) => RoleDiffStatus::OnlyInCurrent,
                (None, Some(_)) => RoleDiffStatus::OnlyInUpstream,
                (None, None) => unreachable!(),
            };
            RoleDiff {
                role_key: key,
                status,
                current: cur,
                upstream: ups,
            }
        })
        .collect();

    // Variables: surface the raw maps so the UI can render a diff
    // view. `differ` is the quick test the UI uses to decide
    // whether to show the variables section at all.
    let current_vars = serde_json::to_value(&current.variables).unwrap_or(serde_json::Value::Null);
    let upstream_vars =
        serde_json::to_value(&upstream.variables).unwrap_or(serde_json::Value::Null);
    let variables_differ = current_vars != upstream_vars;

    PackStarterUpdate {
        pack_key,
        starter_key,
        current_version,
        available_version,
        locally_modified,
        role_diffs,
        variables_differ,
        current_variables: current_vars,
        upstream_variables: upstream_vars,
    }
}

/// Apply the operator's per-role choices to the workspace's pack
/// body. Default-resolution:
///
/// - For roles in both sides, missing action → keep local.
/// - For roles only in upstream, missing action → don't add.
/// - For roles only in current, missing action → keep local.
/// - For variables, missing action ([`VARIABLES_ACTION_KEY`]) → keep local.
///
/// Render-validates the merged body before persisting. Bumps the
/// row's `starter_version` to the upstream version on success —
/// but **does not** clear `locally_modified`. Operators may have
/// chosen `KeepMine` for some roles, which is itself a local
/// decision worth tracking for the next update cycle.
pub async fn apply_update(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    pack_key: &str,
    actions: HashMap<String, UpdateAction>,
) -> ServiceResult<domain_packs::Model> {
    let row = domain_packs::Entity::find()
        .filter(domain_packs::Column::WorkspaceId.eq(workspace_id))
        .filter(domain_packs::Column::PackKey.eq(pack_key))
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;

    let upstream_version = super::starter::version_for(&row.starter_key).ok_or_else(|| {
        ServiceError::InvalidInput(format!("starter `{}` is unknown", row.starter_key))
    })?;
    if upstream_version == row.starter_version {
        return Err(ServiceError::InvalidInput(
            "pack is already at the latest starter version".into(),
        ));
    }
    let upstream_body = super::starter::body_for(&row.starter_key).ok_or_else(|| {
        ServiceError::InvalidInput(format!("starter `{}` has no body", row.starter_key))
    })?;
    let current_body: PackBody = serde_json::from_value(row.body.clone())
        .map_err(|e| ServiceError::InvalidInput(format!("current pack body did not parse: {e}")))?;

    let merged = merge(current_body, upstream_body, &actions);
    super::validate::check_pack_renders(&merged)?;

    let body_json = serde_json::to_value(&merged)
        .map_err(|e| ServiceError::InvalidInput(format!("serialize merged body: {e}")))?;
    use sea_orm::{ActiveModelTrait, Set};
    let mut am: domain_packs::ActiveModel = row.into();
    am.body = Set(body_json);
    am.starter_version = Set(upstream_version.to_string());
    // locally_modified intentionally not cleared — the merged body
    // reflects the operator's choices; some of those choices may
    // be "keep mine" which is a local decision.
    am.updated_at = Set(chrono::Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

fn merge(
    current: PackBody,
    upstream: PackBody,
    actions: &HashMap<String, UpdateAction>,
) -> PackBody {
    let current_roles: BTreeMap<String, RolePrompt> = current
        .roles
        .into_iter()
        .map(|r| (r.role_key.clone(), r))
        .collect();
    let upstream_roles: BTreeMap<String, RolePrompt> = upstream
        .roles
        .into_iter()
        .map(|r| (r.role_key.clone(), r))
        .collect();

    let all_keys: BTreeMap<String, ()> = current_roles
        .keys()
        .chain(upstream_roles.keys())
        .map(|k| (k.clone(), ()))
        .collect();

    let merged_roles: Vec<RolePrompt> = all_keys
        .into_keys()
        .filter_map(|key| {
            let action = actions.get(&key).copied().unwrap_or(UpdateAction::KeepMine);
            match (
                current_roles.get(&key).cloned(),
                upstream_roles.get(&key).cloned(),
                action,
            ) {
                (_, Some(u), UpdateAction::TakeUpstream) => Some(u),
                (Some(c), _, UpdateAction::KeepMine) => Some(c),
                (None, Some(u), UpdateAction::KeepMine) => {
                    // Default for upstream-only roles: don't add.
                    // The operator can explicitly opt in via TakeUpstream.
                    let _ = u;
                    None
                }
                (Some(c), None, UpdateAction::TakeUpstream) => {
                    // Default for current-only roles + TakeUpstream:
                    // means "take upstream's removal." Drop it.
                    let _ = c;
                    None
                }
                (None, None, _) => None,
            }
        })
        .collect();

    let take_upstream_vars = matches!(
        actions.get(VARIABLES_ACTION_KEY).copied(),
        Some(UpdateAction::TakeUpstream)
    );
    let variables = if take_upstream_vars {
        upstream.variables
    } else {
        current.variables
    };

    // Phase A merge policy for `ppe_yolo`: prefer upstream when it
    // ships one, otherwise keep current. This biases toward "starter
    // updates introduce a model" (the common case as the pack
    // catalog grows), and doesn't disturb workspaces that have already
    // customized the field via direct pack edits. A future
    // PPE_YOLO_ACTION_KEY can let the curator UI offer per-pack
    // "keep mine" vs "take upstream" once operators routinely tune
    // the block locally — for v1 nobody does, so we don't need it yet.
    let ppe_yolo = upstream.ppe_yolo.or(current.ppe_yolo);

    PackBody {
        schema_version: upstream.schema_version,
        output_schema: upstream.output_schema,
        variables,
        roles: merged_roles,
        ppe_yolo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn role(key: &str, prompt: &str) -> RolePrompt {
        RolePrompt {
            role_key: key.into(),
            label: key.into(),
            hint: "hint".into(),
            prompt: prompt.into(),
        }
    }

    fn body(roles: Vec<RolePrompt>) -> PackBody {
        let mut vars = BTreeMap::new();
        vars.insert("k".into(), json!("v"));
        PackBody {
            schema_version: 1,
            output_schema: json!({}),
            variables: vars,
            roles,
            ppe_yolo: None,
        }
    }

    #[test]
    fn diff_reports_unchanged_for_identical() {
        let cur = body(vec![role("a", "{{ track_id }}")]);
        let ups = body(vec![role("a", "{{ track_id }}")]);
        let d = diff_pack(
            "p".into(),
            "s".into(),
            "1.0.0".into(),
            "1.0.1".into(),
            false,
            cur,
            ups,
        );
        assert_eq!(d.role_diffs.len(), 1);
        assert_eq!(d.role_diffs[0].status, RoleDiffStatus::Unchanged);
        assert!(!d.variables_differ);
    }

    #[test]
    fn diff_flags_differs_added_removed() {
        let cur = body(vec![role("a", "old"), role("b", "stay")]);
        let ups = body(vec![role("a", "new"), role("c", "new")]);
        let d = diff_pack(
            "p".into(),
            "s".into(),
            "1.0.0".into(),
            "1.0.1".into(),
            true,
            cur,
            ups,
        );
        let by_key: BTreeMap<String, RoleDiffStatus> = d
            .role_diffs
            .iter()
            .map(|r| (r.role_key.clone(), r.status))
            .collect();
        assert_eq!(by_key.get("a"), Some(&RoleDiffStatus::Differs));
        assert_eq!(by_key.get("b"), Some(&RoleDiffStatus::OnlyInCurrent));
        assert_eq!(by_key.get("c"), Some(&RoleDiffStatus::OnlyInUpstream));
    }

    #[test]
    fn merge_take_upstream_replaces_role() {
        let cur = body(vec![role("a", "old")]);
        let ups = body(vec![role("a", "new")]);
        let mut actions = HashMap::new();
        actions.insert("a".into(), UpdateAction::TakeUpstream);
        let merged = merge(cur, ups, &actions);
        assert_eq!(merged.roles.len(), 1);
        assert_eq!(merged.roles[0].prompt, "new");
    }

    #[test]
    fn merge_keep_mine_preserves_local() {
        let cur = body(vec![role("a", "mine")]);
        let ups = body(vec![role("a", "upstream")]);
        let merged = merge(cur, ups, &HashMap::new());
        assert_eq!(merged.roles[0].prompt, "mine");
    }

    #[test]
    fn merge_upstream_only_role_requires_opt_in() {
        let cur = body(vec![]);
        let ups = body(vec![role("new_role", "x")]);
        // Default KeepMine → don't add upstream-only roles.
        let merged_default = merge(cur.clone(), ups.clone(), &HashMap::new());
        assert!(merged_default.roles.is_empty());
        // TakeUpstream → add it.
        let mut actions = HashMap::new();
        actions.insert("new_role".into(), UpdateAction::TakeUpstream);
        let merged_take = merge(cur, ups, &actions);
        assert_eq!(merged_take.roles.len(), 1);
    }

    #[test]
    fn merge_current_only_role_take_upstream_drops_it() {
        let cur = body(vec![role("old_role", "x")]);
        let ups = body(vec![]);
        let mut actions = HashMap::new();
        actions.insert("old_role".into(), UpdateAction::TakeUpstream);
        let merged = merge(cur, ups, &actions);
        assert!(
            merged.roles.is_empty(),
            "TakeUpstream on removed role drops it"
        );
    }

    #[test]
    fn merge_variables_action_controls_replacement() {
        let mut cur_vars = BTreeMap::new();
        cur_vars.insert("local_only".into(), json!("L"));
        let mut ups_vars = BTreeMap::new();
        ups_vars.insert("upstream_only".into(), json!("U"));

        let cur = PackBody {
            schema_version: 1,
            output_schema: json!({}),
            variables: cur_vars.clone(),
            roles: vec![],
            ppe_yolo: None,
        };
        let ups = PackBody {
            schema_version: 1,
            output_schema: json!({}),
            variables: ups_vars.clone(),
            roles: vec![],
            ppe_yolo: None,
        };

        // Default → keep local variables.
        let merged_default = merge(cur.clone(), ups.clone(), &HashMap::new());
        assert!(merged_default.variables.contains_key("local_only"));
        assert!(!merged_default.variables.contains_key("upstream_only"));

        // Take upstream → swap.
        let mut actions = HashMap::new();
        actions.insert(VARIABLES_ACTION_KEY.into(), UpdateAction::TakeUpstream);
        let merged_take = merge(cur, ups, &actions);
        assert!(!merged_take.variables.contains_key("local_only"));
        assert!(merged_take.variables.contains_key("upstream_only"));
    }
}
