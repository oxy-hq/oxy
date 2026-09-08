//! Operator-facing alert delivery (camera health, fleet incidents).
//!
//! Today: one channel — Slack via `chat.postMessage` against the
//! workspace's parent-org's bot installation. Channel selection
//! falls back through `slack_channel_defaults` (per-workspace) →
//! the bot's default channel.
//!
//! Tomorrow: a `Notifier` trait so PagerDuty / email / Microsoft
//! Teams plug into the same call sites without conditionals.
//!
//! ## Scope of this module
//!
//! - **What it does:** delivers a single alert message synchronously
//!   when called. Test endpoint validates wiring.
//! - **What it does NOT do:** poll `camera_health` for transitions
//!   and emit on its own. That background loop is intentionally
//!   deferred to P1 (see `internal-docs/jwt-cutover.md` follow-ups)
//!   — we want operator-driven test alerts shipped first so the
//!   transition logic has a known-good notifier to call.
//!
//! ## Slack client
//!
//! Uses the shared `oxy-slack-client` crate (a leaf: `reqwest` +
//! `oxy-shared`), not the app crate's copy. `oxy-app` depends on
//! `oxy-cameras`, so importing the client back from the app crate
//! would be a cycle — the shared crate sits below both, so both
//! depend on it cleanly. (Formerly this module hand-rolled ~40 lines
//! of `chat.postMessage` plumbing to dodge that cycle.)

use std::collections::HashMap;
use std::time::{Duration, Instant};

use entity::{slack_channel_defaults, slack_installations, workspaces};
use oxy_platform::secrets::OrgSecretsService;
use oxy_slack_client::SlackClient;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::entities::sites;
use crate::service::{ServiceError, ServiceResult, camera_health};

/// What we send for a camera-health degradation. Kept tiny so the
/// payload is legible in tests and Slack-side rendering. Markdown
/// is fine — `chat.postMessage` interprets `mrkdwn: true` by default.
#[derive(Debug, Clone, Serialize)]
pub struct AlertPayload {
    /// Short one-line summary. Becomes the message text.
    pub title: String,
    /// Optional secondary line — usually the camera name + site.
    pub detail: Option<String>,
    /// Optional link to the relevant page in the Edge UI.
    pub url: Option<String>,
}

/// Resolve the destination Slack installation + channel for a
/// workspace. Returns `None` when:
///   - The workspace has no parent org (local-mode workspaces).
///   - The org has no active Slack installation.
///   - There's no default channel configured for the workspace AND
///     no fallback channel for the installation.
///
/// `None` is the correct "do nothing" signal — alert delivery is
/// best-effort and a missing Slack install is a normal state, not
/// an error.
async fn resolve_target(
    db: &DatabaseConnection,
    workspace_id: Uuid,
) -> ServiceResult<Option<(String, String)>> {
    let ws = workspaces::Entity::find_by_id(workspace_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    let Some(org_id) = ws.org_id else {
        return Ok(None);
    };

    let inst = slack_installations::Entity::find()
        .filter(slack_installations::Column::OrgId.eq(org_id))
        .filter(slack_installations::Column::RevokedAt.is_null())
        .one(db)
        .await?;
    let Some(inst) = inst else {
        return Ok(None);
    };

    let bot_token = OrgSecretsService::get_by_id(inst.bot_token_secret_id)
        .await
        .map_err(|e| ServiceError::Internal(format!("open slack bot token: {e}")))?;

    let channel = slack_channel_defaults::Entity::find()
        .filter(slack_channel_defaults::Column::InstallationId.eq(inst.id))
        .filter(slack_channel_defaults::Column::WorkspaceId.eq(workspace_id))
        .one(db)
        .await?
        .map(|c| c.slack_channel_id);
    let Some(channel) = channel else {
        // No per-workspace default. The operator UI surfaces a
        // "Configure default channel" CTA in the Slack settings;
        // until they do, we stay silent rather than picking an
        // arbitrary channel for them.
        return Ok(None);
    };

    Ok(Some((bot_token, channel)))
}

/// POST `chat.postMessage`. Best-effort — surfaces a structured
/// error so callers can decide whether to retry or log, but never
/// blocks the operational path that triggered the alert.
pub async fn notify(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    payload: &AlertPayload,
) -> ServiceResult<bool> {
    let Some((bot_token, channel)) = resolve_target(db, workspace_id).await? else {
        return Ok(false);
    };

    // `chat_post_message` surfaces HTTP, JSON, and Slack's `ok: false`
    // logical failures as one `OxyError` (see `oxy_slack_client`). `mrkdwn`
    // is Slack's default for `chat.postMessage`, so we needn't set it.
    SlackClient::new()
        .chat_post_message(&bot_token, &channel, &build_alert_text(payload), None)
        .await
        .map_err(|e| ServiceError::Upstream(format!("slack chat.postMessage: {e}")))?;
    Ok(true)
}

/// Build the alert message text. Extracted for unit testing without
/// hitting Slack. Markdown is fine — `chat.postMessage` treats `mrkdwn`
/// as true by default.
fn build_alert_text(payload: &AlertPayload) -> String {
    let mut text = format!("⚠️ *{}*", payload.title);
    if let Some(detail) = &payload.detail {
        text.push('\n');
        text.push_str(detail);
    }
    if let Some(url) = &payload.url {
        text.push_str(&format!("\n<{url}|Open Edge dashboard>"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alert_text_includes_title_detail_url() {
        let text = build_alert_text(&AlertPayload {
            title: "Camera offline".into(),
            detail: Some("Kitchen-1 @ Site A".into()),
            url: Some("https://oxy.example.com/edge".into()),
        });
        assert!(text.contains("Camera offline"));
        assert!(text.contains("Kitchen-1 @ Site A"));
        assert!(text.contains("https://oxy.example.com/edge"));
    }

    #[test]
    fn alert_text_omits_optional_fields_when_none() {
        let text = build_alert_text(&AlertPayload {
            title: "Heartbeat".into(),
            detail: None,
            url: None,
        });
        assert!(text.contains("Heartbeat"));
        assert!(!text.contains("Open Edge dashboard"));
    }
}

// ── Background poller (P1 #2) ──────────────────────────────────────────────
//
// Detects camera-health transitions and emits a Slack message per
// transition, with cooldown to avoid spam. State is kept in-memory
// per process; a restart re-emits "current state is X" for everything
// after the startup grace window, which is acceptable since (a) a
// real production restart is rare, and (b) the grace window stops
// the storm.
//
// The whole poller is a no-op when no workspace has a Slack
// installation — `alerts::notify` returns `delivered=false` and the
// transition is logged but otherwise silent.

/// Tick cadence. 60s is a good balance: tighter than this and we
/// hammer the camera_health rollup query for every workspace every
/// second; looser and the operator's "the kitchen camera went out"
/// signal is delayed past the point of usefulness.
pub const ALERTER_TICK_INTERVAL: Duration = Duration::from_secs(60);

/// Don't emit alerts during the first N seconds of process lifetime.
/// On a fresh boot we have no previous-tick state, so every camera
/// looks like a transition. Suppressing the first window means a
/// rolling deploy doesn't generate a wall of "kitchen-1 is degraded"
/// messages just because we lost in-memory state.
const STARTUP_GRACE: Duration = Duration::from_secs(300);

/// Per-camera cooldown between alerts. Flapping cameras shouldn't
/// page the operator twenty times in an hour; one signal per
/// transition per cooldown window is enough.
const COOLDOWN: Duration = Duration::from_secs(30 * 60);

/// In-process state for the alert loop. Two maps because transition
/// detection and cooldown are different concerns: a camera can
/// transition (status changed) but we suppress the alert (cooldown
/// hasn't elapsed), and we want to keep the *new* status in
/// `last_status` for the next tick's diff regardless.
#[derive(Debug, Default)]
struct AlertState {
    /// When each workspace was last warned about a failing health summary;
    /// repeats inside `SUMMARIZE_FAILURE_WARN_EVERY` are logged at `debug`.
    last_summarize_warn_at: HashMap<Uuid, Instant>,
    started_at: Option<Instant>,
    last_status: HashMap<(Uuid, Uuid), String>,
    last_alert_at: HashMap<(Uuid, Uuid), Instant>,
}

/// Spawn the alerter on the current Tokio runtime. Same shutdown
/// contract as `stale::spawn` / `rollouts::spawn` so it exits cleanly
/// on SIGTERM with the rest of the camera-domain loops.
pub fn spawn(db: DatabaseConnection, shutdown: CancellationToken) {
    tokio::spawn(async move {
        info!(
            tick_secs = ALERTER_TICK_INTERVAL.as_secs(),
            "cameras.alerter: started"
        );
        let mut state = AlertState {
            started_at: Some(Instant::now()),
            ..Default::default()
        };
        let mut ticker = tokio::time::interval(ALERTER_TICK_INTERVAL);
        ticker.tick().await; // skip immediate tick
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    info!("cameras.alerter: shutdown");
                    return;
                }
                _ = ticker.tick() => {
                    if let Err(e) = alerter_tick(&db, &mut state).await {
                        warn!(error = %e, "cameras.alerter: tick failed");
                    }
                }
            }
        }
    });
}

/// How long one workspace's `summarize failed` stays at `debug` after a `warn`.
const SUMMARIZE_FAILURE_WARN_EVERY: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// The throttle decision over the state's map: deterministic given the map
/// and `now`, which is what the test relies on.
fn warn_is_due(
    last: &mut std::collections::HashMap<Uuid, Instant>,
    workspace_id: Uuid,
    now: Instant,
) -> bool {
    match last.get(&workspace_id) {
        Some(prev) if now.duration_since(*prev) < SUMMARIZE_FAILURE_WARN_EVERY => false,
        _ => {
            last.insert(workspace_id, now);
            true
        }
    }
}

/// One pass of the alert detector. Public so tests can drive it
/// deterministically.
pub async fn alerter_tick(db: &DatabaseConnection, state: &mut AlertState) -> ServiceResult<()> {
    let now = Instant::now();
    let in_grace = state
        .started_at
        .map(|t| now.duration_since(t) < STARTUP_GRACE)
        .unwrap_or(false);

    // Find every workspace that owns at least one site — the rollup
    // query is cheap per workspace (one indexed scan) but we don't
    // want to fire it for tenants with no fleet at all.
    let workspace_ids: Vec<Uuid> = sites::Entity::find()
        .select_only()
        .column(sites::Column::WorkspaceId)
        .into_tuple::<Uuid>()
        .all(db)
        .await?
        .into_iter()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    for workspace_id in workspace_ids {
        let summary = match camera_health::summarize_for_workspace(db, workspace_id).await {
            Ok(rows) => rows,
            Err(e) => {
                // Once an hour per workspace at `warn`; the tick is every
                // few seconds and a workspace without an Airhouse tenant
                // fails the same way on every one of them (measured: ~530
                // identical warns an hour on oxy-dev).
                if warn_is_due(&mut state.last_summarize_warn_at, workspace_id, now) {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %e,
                        warn_throttled_secs = SUMMARIZE_FAILURE_WARN_EVERY.as_secs(),
                        "alerter: summarize failed"
                    );
                } else {
                    tracing::debug!(workspace_id = %workspace_id, error = %e, "alerter: summarize failed");
                }
                continue;
            }
        };
        for row in summary {
            // `unknown` rows mean the worker has never reported; treat
            // as no-signal rather than a transition. Without this guard
            // a fresh camera (no health row) would emit "ok → unknown"
            // → "unknown → degraded" noise during onboarding.
            if row.status == "unknown" {
                state
                    .last_status
                    .insert((workspace_id, row.camera_id), row.status);
                continue;
            }

            let key = (workspace_id, row.camera_id);
            let previous = state.last_status.get(&key).cloned();
            state.last_status.insert(key, row.status.clone());

            // No previous status: this is the first time we've seen
            // the camera. Record it but don't alert — we have nothing
            // to compare against.
            let Some(prev) = previous else {
                continue;
            };
            if prev == row.status {
                continue;
            }
            if !should_alert_on_transition(&prev, &row.status) {
                continue;
            }
            if in_grace {
                continue;
            }
            if let Some(last_at) = state.last_alert_at.get(&key)
                && now.duration_since(*last_at) < COOLDOWN
            {
                continue;
            }

            let payload = build_transition_payload(&prev, &row);
            // Persist the transition to audit_events BEFORE attempting
            // Slack delivery. Reason: the Dashboard's "Recent alerts"
            // feed reads these rows, and workspaces without a Slack
            // install still need a transition history. Delivery success
            // is recorded in the audit details so the feed can show
            // "sent" vs "no Slack" on a per-row basis.
            let delivered = match notify(db, workspace_id, &payload).await {
                Ok(d) => Some(d),
                Err(e) => {
                    warn!(
                        workspace_id = %workspace_id,
                        camera_id = %row.camera_id,
                        error = %e,
                        "alerter: notify failed"
                    );
                    None
                }
            };
            crate::service::audit::record(
                db,
                workspace_id,
                None, // alerter has no acting user
                crate::service::audit::ACTION_CAMERA_ALERT,
                Some(row.camera_id),
                serde_json::json!({
                    "from": prev,
                    "to": row.status,
                    "reason": row.reason,
                    "title": payload.title,
                    "delivered": delivered,
                }),
            )
            .await;

            // Live fan-out (world-model SSE), independent of Slack delivery.
            crate::service::events::emit(
                workspace_id,
                crate::service::events::CameraDomainEvent::HealthTransition {
                    camera_id: row.camera_id,
                    from: prev.clone(),
                    to: row.status.clone(),
                    reason: row.reason.clone(),
                },
            );

            if delivered == Some(true) {
                state.last_alert_at.insert(key, now);
                info!(
                    workspace_id = %workspace_id,
                    camera_id = %row.camera_id,
                    from = prev,
                    to = row.status,
                    "alerter: emitted transition"
                );
            }
            // delivered == Some(false): no Slack install. Audit row
            // still landed above — the operator UI surfaces it.
            // delivered == None: hard error already logged above.
        }
    }
    Ok(())
}

/// Which transitions deserve an alert. Rule of thumb: any change
/// involving `ok` is interesting (degradation or recovery); pure
/// non-ok → non-ok transitions are escalations but rare enough that
/// the existing alert from the FIRST transition is enough signal.
fn should_alert_on_transition(prev: &str, next: &str) -> bool {
    prev == "ok" || next == "ok"
}

fn build_transition_payload(prev: &str, row: &camera_health::CameraHealthSummary) -> AlertPayload {
    let detail = if !row.reason.is_empty() {
        format!("Was {prev}, now {} — {}", row.status, row.reason)
    } else {
        format!("Was {prev}, now {}", row.status)
    };
    AlertPayload {
        title: format!(
            "Camera {} → {}",
            &row.camera_id.to_string()[..8],
            row.status
        ),
        detail: Some(detail),
        url: None,
    }
}

// ── Recent-alerts feed ────────────────────────────────────────────────────
//
// Dashboard "Recent alerts" panel reads from audit_events filtered
// by `action = 'camera.alert'`. Done here (not in `service::audit`)
// because the enrichment needs camera + site name lookups against
// THIS crate's entities, and the audit module is intentionally
// generic / aggregate-agnostic.

/// One row as the Dashboard renders it. Camera + site names are
/// pre-resolved Postgres-side so the UI doesn't fan out per-row
/// to learn what a UUID refers to.
#[derive(Debug, Clone, Serialize)]
pub struct RecentAlertRow {
    pub id: Uuid,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub camera_id: Option<Uuid>,
    pub camera_name: Option<String>,
    pub site_id: Option<Uuid>,
    pub site_name: Option<String>,
    pub from: String,
    pub to: String,
    pub reason: String,
    /// `Some(true)` = Slack delivered, `Some(false)` = no Slack
    /// install / no default channel, `None` = notify call errored.
    /// Surfaces in the UI as a small pill.
    pub delivered: Option<bool>,
}

/// Most-recent-first list of camera health transitions for the
/// workspace. Optional `site_id` narrows to a site by joining
/// camera → site in-memory after the audit fetch (cheaper than a
/// SQL join because the audit table is workspace-scoped).
pub async fn recent_alerts(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    site_id: Option<Uuid>,
    limit: u32,
) -> ServiceResult<Vec<RecentAlertRow>> {
    use crate::entities::cameras;

    let raw = crate::service::audit::list_for_workspace(
        db,
        workspace_id,
        crate::service::audit::ListFilter {
            action_prefix: Some(crate::service::audit::ACTION_CAMERA_ALERT.to_string()),
            target_id: None,
            limit: Some(limit.clamp(1, 200)),
            offset: None,
        },
    )
    .await?;
    if raw.is_empty() {
        return Ok(Vec::new());
    }

    // Resolve camera + site names in two batched lookups so the
    // enriched feed is one Postgres query per join, not one per row.
    let camera_ids: Vec<Uuid> = raw.iter().filter_map(|r| r.target_id).collect();
    let cam_rows = if camera_ids.is_empty() {
        Vec::new()
    } else {
        cameras::Entity::find()
            .filter(cameras::Column::Id.is_in(camera_ids))
            .all(db)
            .await?
    };
    let cam_by_id: HashMap<Uuid, &crate::entities::cameras::Model> =
        cam_rows.iter().map(|c| (c.id, c)).collect();

    let site_ids: Vec<Uuid> = cam_rows.iter().map(|c| c.site_id).collect();
    let site_rows = if site_ids.is_empty() {
        Vec::new()
    } else {
        sites::Entity::find()
            .filter(sites::Column::Id.is_in(site_ids))
            .all(db)
            .await?
    };
    let site_by_id: HashMap<Uuid, &crate::entities::sites::Model> =
        site_rows.iter().map(|s| (s.id, s)).collect();

    let mut out = Vec::with_capacity(raw.len());
    for row in raw {
        let cam = row.target_id.and_then(|cid| cam_by_id.get(&cid).copied());
        let site = cam.and_then(|c| site_by_id.get(&c.site_id).copied());
        if let Some(filter) = site_id
            && site.map(|s| s.id) != Some(filter)
        {
            continue;
        }
        let d = &row.details_json;
        out.push(RecentAlertRow {
            id: row.id,
            created_at: row.created_at,
            camera_id: row.target_id,
            camera_name: cam.map(|c| c.name.clone()),
            site_id: site.map(|s| s.id),
            site_name: site.map(|s| s.name.clone()),
            from: d
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            to: d
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            reason: d
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            delivered: d.get("delivered").and_then(|v| v.as_bool()),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod transition_tests {
    use super::*;

    #[test]
    fn alerts_on_ok_to_degraded() {
        assert!(should_alert_on_transition("ok", "degraded"));
    }
    #[test]
    fn alerts_on_recovery() {
        assert!(should_alert_on_transition("degraded", "ok"));
        assert!(should_alert_on_transition("stale", "ok"));
    }
    #[test]
    fn no_alert_on_non_ok_escalation() {
        // Operator already got the first signal when ok → degraded.
        // degraded → stale is "still bad," not a new problem.
        assert!(!should_alert_on_transition("degraded", "stale"));
    }
}

#[cfg(test)]
mod summarize_throttle_tests {
    use super::*;

    #[test]
    fn a_workspace_is_warned_about_once_an_hour() {
        let mut last = std::collections::HashMap::new();
        let ws = Uuid::new_v4();
        let other = Uuid::new_v4();
        let t0 = Instant::now();
        assert!(warn_is_due(&mut last, ws, t0), "first failure warns");
        assert!(!warn_is_due(
            &mut last,
            ws,
            t0 + std::time::Duration::from_secs(5)
        ));
        assert!(!warn_is_due(
            &mut last,
            ws,
            t0 + std::time::Duration::from_secs(59 * 60)
        ));
        assert!(
            warn_is_due(&mut last, other, t0),
            "another workspace is independent"
        );
        assert!(warn_is_due(
            &mut last,
            ws,
            t0 + SUMMARIZE_FAILURE_WARN_EVERY + std::time::Duration::from_secs(1)
        ));
    }
}
