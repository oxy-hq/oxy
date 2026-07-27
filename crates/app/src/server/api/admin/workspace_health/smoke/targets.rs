//! What to probe: turning a `SmokeTestConfig` into a concrete list of targets.
//!
//! Split from `runner.rs` so the two halves stay separable — this module decides
//! *what* gets probed (enumerating, resolving named selections, applying the
//! per-kind cap), while the runner decides *how* (concurrency, timeouts,
//! verdicts). It is pure apart from reading the workspace context and the
//! compiled revision, which is what makes the selection rules unit-testable
//! without a warehouse.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxy::config::health_check::{
    AppProbeTarget, AppsProbeConfig, SemanticProbeConfig, SmokeTestConfig,
};

use super::probes::{self, SemanticTarget};
use super::{SmokeProbeKind, SmokeVerdict, failed, note, unavailable};
use crate::agentic_wiring::project_ctx::OxyProjectContext;
use crate::server::api::compiled_reader::{list_semantic_topics, list_semantic_views};

/// One thing to probe. `kind`/`label` drive the verdict's identity, so they are
/// derived here rather than threaded through every probe.
pub(super) enum Target {
    Connection(String),
    Semantic(SemanticTarget),
    App {
        path: PathBuf,
        variables: HashMap<String, serde_json::Value>,
    },
    Agent {
        /// Pre-resolved and de-duplicated by `SmokeTestConfig::resolved_agents`,
        /// so several prompts against one agent stay distinct in the payload.
        label: String,
        agent_ref: String,
        prompt: String,
    },
}

impl Target {
    pub(super) fn kind(&self) -> SmokeProbeKind {
        match self {
            Target::Connection(_) => SmokeProbeKind::Connection,
            Target::Semantic(_) => SmokeProbeKind::Semantic,
            Target::App { .. } => SmokeProbeKind::App,
            Target::Agent { .. } => SmokeProbeKind::Agent,
        }
    }

    pub(super) fn label(&self) -> String {
        match self {
            Target::Connection(db) => db.clone(),
            Target::Semantic(t) => t.topic.clone(),
            Target::App { path, .. } => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            Target::Agent { label, .. } => label.clone(),
        }
    }
}

/// Enumerate everything the config asks us to probe, capped per kind. Returns
/// the targets plus any Healthy notes (skipped topics, truncated lists) that
/// should appear in the checks table regardless of what the probes find.
pub(super) async fn collect_targets(
    ctx: &Arc<OxyProjectContext>,
    workspace_id: uuid::Uuid,
    cfg: &SmokeTestConfig,
) -> (Vec<Target>, Vec<SmokeVerdict>) {
    let mut targets = Vec::new();
    let mut notes = Vec::new();
    let max = cfg.max_targets();

    if cfg.connections {
        let names: Vec<String> = ctx
            .workspace_manager()
            .config_manager
            .list_databases()
            .into_iter()
            .map(|d| d.name)
            .collect();
        let (kept, note) = cap(SmokeProbeKind::Connection, names, max, "databases");
        notes.extend(note);
        targets.extend(kept.into_iter().map(Target::Connection));
    }

    if cfg.semantic.enabled() {
        match semantic_targets(workspace_id, &cfg.semantic, max).await {
            Ok((kept, mut semantic_notes)) => {
                notes.append(&mut semantic_notes);
                targets.extend(kept.into_iter().map(Target::Semantic));
            }
            Err(reason) => notes.push(unavailable(SmokeProbeKind::Semantic, "topics", reason)),
        }
    }

    if cfg.apps.enabled() {
        let (kept, mut app_notes) = app_targets(ctx, &cfg.apps, max).await;
        notes.append(&mut app_notes);
        targets.extend(kept);
    }

    targets.extend(cfg.resolved_agents().into_iter().map(|a| Target::Agent {
        label: a.label,
        agent_ref: a.agent_ref,
        prompt: a.prompt,
    }));

    (targets, notes)
}

/// Pair topics with a measure to query — every compiled topic for a sweep, or
/// just the named ones for a selection. `Err` when there is no compiled revision
/// to read topics from (a draft branch on a non-serve node).
async fn semantic_targets(
    workspace_id: uuid::Uuid,
    cfg: &SemanticProbeConfig,
    max: usize,
) -> Result<(Vec<SemanticTarget>, Vec<SmokeVerdict>), String> {
    let topics = list_semantic_topics(workspace_id, None)
        .await
        .map_err(|e| format!("could not read topics: {e}"))?
        .ok_or_else(|| "no compiled revision to read topics from".to_string())?;
    let views = list_semantic_views(workspace_id, None)
        .await
        .map_err(|e| format!("could not read views: {e}"))?
        .unwrap_or_default();

    let plan = match cfg.selection() {
        Some(selection) => probes::plan_selected(selection, &topics, &views),
        None => probes::plan(&topics, &views),
    };

    let mut notes: Vec<SmokeVerdict> = plan
        .skipped
        .into_iter()
        .map(|reason| note(SmokeProbeKind::Semantic, "topics", reason))
        .collect();
    // A topic the config named but we could not resolve is the workspace being
    // wrong, not us — Unhealthy, the same as a measure that fails to run.
    notes.extend(
        plan.missing
            .into_iter()
            .map(|m| failed(SmokeProbeKind::Semantic, m.topic, m.reason, 0)),
    );
    let (kept, cap_note) = cap(SmokeProbeKind::Semantic, plan.targets, max, "topics");
    notes.extend(cap_note);
    Ok((kept, notes))
}

/// Enumerate the apps to run — every `.app.yml` for a sweep, or just the named
/// ones for a selection.
async fn app_targets(
    ctx: &Arc<OxyProjectContext>,
    cfg: &AppsProbeConfig,
    max: usize,
) -> (Vec<Target>, Vec<SmokeVerdict>) {
    let available = match ctx.workspace_manager().config_manager.list_apps().await {
        Ok(paths) => paths,
        Err(e) => {
            return (
                Vec::new(),
                vec![unavailable(
                    SmokeProbeKind::App,
                    "apps",
                    format!("could not list apps: {e}"),
                )],
            );
        }
    };

    // A sweep runs every app on its own defaults; a selection carries each app's
    // own control values.
    let (chosen, mut notes) = match cfg.selection() {
        Some(selection) => select_apps(selection, &available),
        None => (
            available
                .into_iter()
                .map(|path| (path, HashMap::new()))
                .collect(),
            Vec::new(),
        ),
    };

    let (kept, cap_note) = cap(SmokeProbeKind::App, chosen, max, "apps");
    notes.extend(cap_note);
    let targets = kept
        .into_iter()
        .map(|(path, variables)| Target::App { path, variables })
        .collect();
    (targets, notes)
}

/// Resolve each selection entry against the workspace's apps, pairing it with
/// its own control values.
///
/// An entry that matches nothing is Unhealthy, mirroring the semantic selection:
/// the workspace asserted the app should be probed, so quietly running nothing
/// would be a false OK — and a renamed app is exactly the drift worth catching.
fn select_apps(
    selection: &[AppProbeTarget],
    available: &[PathBuf],
) -> (Vec<AppRun>, Vec<SmokeVerdict>) {
    let mut chosen = Vec::new();
    let mut notes = Vec::new();

    for want in selection {
        match available.iter().find(|p| path_matches(p, &want.app)) {
            Some(path) => chosen.push((
                path.clone(),
                want.variables
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            )),
            None => notes.push(failed(
                SmokeProbeKind::App,
                want.app.clone(),
                format!(
                    "app '{}' is named in health_check.smoke_test.apps but does not exist in this \
                     workspace",
                    want.app
                ),
                0,
            )),
        }
    }
    (chosen, notes)
}

/// One resolved app run: which file, and the control values to run it with.
type AppRun = (PathBuf, HashMap<String, serde_json::Value>);

/// Whether an app path satisfies a selection entry, matched on whole trailing
/// path components. `list_apps` may return absolute paths, so a plain string
/// compare would never hit; comparing components (rather than a raw string
/// suffix) keeps `sales.app.yml` from matching `regional_sales.app.yml`.
fn path_matches(path: &Path, entry: &str) -> bool {
    let wanted: Vec<_> = Path::new(entry).components().collect();
    if wanted.is_empty() {
        return false;
    }
    let actual: Vec<_> = path.components().collect();
    actual.len() >= wanted.len() && actual[actual.len() - wanted.len()..] == wanted[..]
}

/// Truncate to `max` targets, recording what was dropped. A cap is a fact about
/// the workspace's size, not a health problem, so the note is Healthy — but it
/// is never silent, or a partial sweep would read as full coverage.
fn cap<T>(
    kind: SmokeProbeKind,
    mut items: Vec<T>,
    max: usize,
    label: &str,
) -> (Vec<T>, Option<SmokeVerdict>) {
    let total = items.len();
    if total <= max {
        return (items, None);
    }
    items.truncate(max);
    let dropped = total - max;
    (
        items,
        Some(note(
            kind,
            label,
            format!("probed {max} of {total} {label}; skipped {dropped} (max_targets={max})"),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::admin::workspace_health::evaluator::HealthStatus;

    #[test]
    fn cap_under_the_limit_adds_no_note() {
        let (kept, note) = cap(SmokeProbeKind::Connection, vec![1, 2, 3], 25, "databases");
        assert_eq!(kept.len(), 3);
        assert!(note.is_none());
    }

    #[test]
    fn cap_truncates_and_records_what_it_dropped() {
        let (kept, note) = cap(SmokeProbeKind::Semantic, (0..30).collect(), 25, "topics");
        assert_eq!(kept.len(), 25);
        let note = note.expect("truncation must not be silent");
        // Healthy: a big workspace is large, not unhealthy.
        assert_eq!(note.status, HealthStatus::Healthy);
        let reason = note.reason.unwrap();
        assert!(reason.contains("probed 25 of 30 topics"));
        assert!(reason.contains("skipped 5"));
    }

    #[test]
    fn cap_at_exactly_the_limit_is_not_truncation() {
        let (kept, note) = cap(SmokeProbeKind::App, (0..25).collect(), 25, "apps");
        assert_eq!(kept.len(), 25);
        assert!(note.is_none());
    }

    #[test]
    fn target_labels_and_kinds() {
        assert_eq!(
            Target::Connection("bq".into()).kind(),
            SmokeProbeKind::Connection
        );
        assert_eq!(Target::Connection("bq".into()).label(), "bq");
        assert_eq!(
            Target::App {
                path: PathBuf::from("apps/sales.app.yml"),
                variables: HashMap::new(),
            }
            .label(),
            "sales.app.yml",
            "an app is labelled by file name, not full path"
        );
        assert_eq!(
            Target::Semantic(SemanticTarget {
                topic: "sales".into(),
                measures: vec!["orders.net".into()],
            })
            .label(),
            "sales"
        );
        let agent = Target::Agent {
            label: "nightly check".into(),
            agent_ref: "agents/a.agentic.yml".into(),
            prompt: "hi".into(),
        };
        assert_eq!(agent.kind(), SmokeProbeKind::Agent);
        assert_eq!(
            agent.label(),
            "nightly check",
            "the resolved label identifies the probe, not the agent_ref"
        );
    }

    fn want_app(app: &str) -> AppProbeTarget {
        AppProbeTarget {
            app: app.to_string(),
            variables: Default::default(),
        }
    }

    #[test]
    fn an_app_entry_matches_on_whole_path_components() {
        let available = vec![
            PathBuf::from("/srv/ws/apps/sales.app.yml"),
            PathBuf::from("/srv/ws/apps/regional_sales.app.yml"),
        ];
        // A bare file name and a relative path both resolve against an absolute
        // listing.
        let (chosen, notes) = select_apps(&[want_app("sales.app.yml")], &available);
        assert!(notes.is_empty());
        assert_eq!(chosen[0].0, PathBuf::from("/srv/ws/apps/sales.app.yml"));
        assert_eq!(chosen.len(), 1);
        assert!(
            !path_matches(
                Path::new("/srv/ws/apps/regional_sales.app.yml"),
                "sales.app.yml"
            ),
            "a raw string suffix would wrongly match regional_sales.app.yml"
        );

        let (chosen, notes) = select_apps(&[want_app("apps/sales.app.yml")], &available);
        assert!(notes.is_empty());
        assert_eq!(chosen.len(), 1);
    }

    #[test]
    fn each_app_carries_its_own_variables() {
        // Variables are per-app, not shared across the selection — two apps
        // rarely want the same control values.
        let available = vec![
            PathBuf::from("/srv/ws/apps/sales.app.yml"),
            PathBuf::from("/srv/ws/apps/inventory.app.yml"),
        ];
        let selection = vec![
            AppProbeTarget {
                app: "sales.app.yml".into(),
                variables: [("region".to_string(), serde_json::json!("us-east"))]
                    .into_iter()
                    .collect(),
            },
            want_app("inventory.app.yml"),
        ];
        let (chosen, notes) = select_apps(&selection, &available);
        assert!(notes.is_empty());
        assert_eq!(chosen[0].1["region"], serde_json::json!("us-east"));
        assert!(
            chosen[1].1.is_empty(),
            "the second app must not inherit the first's variables"
        );
    }

    #[test]
    fn a_named_app_that_does_not_exist_is_unhealthy() {
        // Same false-OK guard as the semantic selection: the workspace asked for
        // this app by name, so running nothing must not read clear.
        let available = vec![PathBuf::from("/srv/ws/apps/sales.app.yml")];
        let (chosen, notes) = select_apps(&[want_app("renamed.app.yml")], &available);
        assert!(chosen.is_empty());
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].status, HealthStatus::Unhealthy);
        assert_eq!(notes[0].target, "renamed.app.yml");
        assert!(notes[0].reason.as_ref().unwrap().contains("does not exist"));
    }
}
