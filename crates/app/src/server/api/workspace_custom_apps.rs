//! `GET /api/{workspace_id}/custom-apps` — published custom apps
//! for the current workspace.
//!
//! Powers the workspace sidebar's "Custom Apps" section. Mounted
//! inside the workspace router so the workspace_middleware handles
//! org-membership authorization — anyone who can see the workspace
//! can list its published custom apps.
//!
//! Drafts are intentionally absent. Oxy staff iterating on an
//! unpublished app reach it via `/admin/apps`, not the customer
//! sidebar.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use entity::organizations;
use entity::prelude::{Apps, Organizations};
use entity::{apps, apps::Model as AppsModel};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::OptionalUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::custom_apps_manifest::OxyAppManifest;
use oxy_shared::utils::custom_app_url::build_pretty_url;

#[derive(Deserialize)]
pub struct WorkspaceIdPath {
    pub workspace_id: Uuid,
}

#[derive(Serialize)]
pub struct CustomAppSummary {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub org_slug: String,
    /// Canonical URL the sidebar links to. Same-tab navigation lands
    /// the user on the bespoke app surface (no embed, no workspace
    /// chrome — these apps own their own UX).
    pub url: String,
    pub published_at: String,
    /// One-line purpose for the launcher card (from the app manifest).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Agent ref the Ask overlay binds to for this app (manifest `ask.agent`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    /// Launcher-card chips (manifest `ask.suggestedQuestions`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggested_questions: Vec<String>,
    /// Absolute (same-origin) URL of the card art, when the manifest
    /// declares a safe relative `art` path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub art_url: Option<String>,
    /// Absolute (same-origin) URL of the shell-rail icon, when the
    /// manifest declares a safe relative `icon` path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// Launcher-card status line (manifest `status`) — a plain display
    /// string, e.g. "23 stores · sales +33.5% YoY · live".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// `"org"` or `"members"` — what the launcher needs to show an org officer
    /// this app's access state, and to offer the control that changes it,
    /// without a second request per card.
    ///
    /// Normalized through [`AppsModel::is_restricted`] rather than passed
    /// through raw, so the wire contract stays the two documented values and
    /// an unrecognized column reads as unrestricted on both sides of the
    /// wire — the same way the access gates read it.
    pub visibility: String,
}

/// Art must be a plain relative path inside the bundle — reject anything
/// that could escape the bundle dir or point off-origin.
///
/// `pub(crate)` so the seeded example bundle is checked by THIS predicate
/// rather than a copy of it (`cli::commands::seed_apps`): a copy would drift,
/// and then the example would either break silently or fail on a rule the
/// launcher doesn't actually apply.
pub(crate) fn safe_relative_art_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains("..")
        && !p.contains("://")
        && !p.contains('\\')
        && !p.contains('?')
        && !p.contains('#')
}

/// Card metadata resolved from a manifest. Everything defaults to empty
/// when the manifest is missing or unreadable so the summary endpoint
/// never fails on a metadata problem. `art`/`icon` are sanitized relative
/// paths (resolved to URLs by the caller); the rest are plain display data.
#[derive(Default)]
struct CardFields {
    description: Option<String>,
    default_agent: Option<String>,
    suggested_questions: Vec<String>,
    art: Option<String>,
    icon: Option<String>,
    status: Option<String>,
}

/// Resolve a manifest's `icon`/`art` relative paths to same-origin URLs under
/// the app's canonical bundle URL, returning `(icon_url, art_url)`. Shared by
/// the homepage launcher list and the admin apps list so every surface shows
/// the same picture from the one manifest source. See the
/// `oxy-app-visual-identity` skill.
pub(super) fn icon_art_urls(
    manifest: Option<&OxyAppManifest>,
    org_slug: &str,
    app_slug: &str,
) -> (Option<String>, Option<String>) {
    let Some(m) = manifest else {
        return (None, None);
    };
    // Trailing slash matches `build_pretty_url`, so the relative path appends
    // to a valid same-origin URL.
    let base = format!("/customer-apps/{org_slug}/{app_slug}/");
    let to_url = |p: &str| format!("{base}{p}");
    let icon = m
        .icon
        .as_deref()
        .filter(|p| safe_relative_art_path(p))
        .map(to_url);
    let art = m
        .art
        .as_deref()
        .filter(|p| safe_relative_art_path(p))
        .map(to_url);
    (icon, art)
}

fn manifest_card_fields(manifest: Option<&OxyAppManifest>) -> CardFields {
    let Some(m) = manifest else {
        return CardFields::default();
    };
    CardFields {
        description: m.description.clone(),
        default_agent: m.ask.as_ref().and_then(|a| a.agent.clone()),
        suggested_questions: m
            .ask
            .as_ref()
            .map(|a| a.suggested_questions.clone())
            .unwrap_or_default(),
        art: m
            .art
            .as_deref()
            .filter(|p| safe_relative_art_path(p))
            .map(str::to_string),
        icon: m
            .icon
            .as_deref()
            .filter(|p| safe_relative_art_path(p))
            .map(str::to_string),
        status: m.status.clone(),
    }
}

impl CustomAppSummary {
    fn from_model_with_manifest(
        m: AppsModel,
        org_slug: &str,
        published_at: String,
        manifest: Option<&OxyAppManifest>,
    ) -> Self {
        let url = build_pretty_url(org_slug, &m.slug);
        let fields = manifest_card_fields(manifest);
        // `url` ends with a trailing slash (e.g. "/customer-apps/acme/my-app/"),
        // so appending the relative art/icon path yields a valid same-origin URL.
        let art_url = fields.art.map(|a| format!("{url}{a}"));
        let icon_url = fields.icon.map(|i| format!("{url}{i}"));
        let visibility = if m.is_restricted() { "members" } else { "org" };
        Self {
            id: m.id,
            slug: m.slug,
            name: m.name,
            org_slug: org_slug.to_string(),
            url,
            published_at,
            description: fields.description,
            default_agent: fields.default_agent,
            suggested_questions: fields.suggested_questions,
            art_url,
            icon_url,
            status: fields.status,
            visibility: visibility.to_string(),
        }
    }
}

pub async fn list_custom_apps(
    OptionalUserExtractor(viewer): OptionalUserExtractor,
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
) -> Result<Json<Vec<CustomAppSummary>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_custom_apps DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Optional rather than required: the route is mounted behind auth middleware, so
    // a missing user means the middleware was bypassed — and the fail-closed filter
    // below is a better answer to that than a 401 that hides the misconfiguration.
    let viewer = viewer.as_ref().map(|u| Viewer {
        id: u.id,
        email: u.email.as_deref().unwrap_or(""),
    });
    let out = published_app_summaries(&db, workspace_id, viewer)
        .await
        .map_err(|e| {
            tracing::error!("list_custom_apps failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(out))
}

/// Who is asking, for the visibility filter in [`published_app_summaries`].
///
/// `None` at the call site means "nobody authenticated", which drops every
/// restricted app rather than 401ing — see the handler above.
#[derive(Clone, Copy)]
pub struct Viewer<'a> {
    pub id: Uuid,
    pub email: &'a str,
}

/// Published-app summaries for a workspace, **filtered to what `viewer` may open**.
/// Shared by the workspace sidebar endpoint above and the custom-app shell-context
/// endpoint (`custom_apps_shell_context.rs`) so the two surfaces can't drift on
/// which apps are listed or how their URLs/icons resolve.
///
/// A restricted app the viewer holds no grant on is **omitted**, not rendered
/// locked: the launcher would otherwise show a card that 403s on click, and the
/// app's very name is often the thing being restricted.
///
/// The filter asks `oxy-authz` rather than re-deriving the visibility rule in SQL —
/// one facts load for the whole page, then pure in-memory set containment per app.
/// Re-stating the rule here is exactly the drift the crate exists to end, and this
/// list is a hot path where an N+1 grant lookup would be felt.
pub async fn published_app_summaries(
    db: &sea_orm::DatabaseConnection,
    workspace_id: Uuid,
    viewer: Option<Viewer<'_>>,
) -> Result<Vec<CustomAppSummary>, sea_orm::DbErr> {
    let rows = Apps::find()
        .filter(apps::Column::ProjectId.eq(workspace_id))
        .filter(apps::Column::PublishedAt.is_not_null())
        .order_by_asc(apps::Column::Name)
        .all(db)
        .await?;

    if rows.is_empty() {
        return Ok(vec![]);
    }

    let any_restricted = rows.iter().any(|a| a.is_restricted());

    // Only load facts when at least one app is actually restricted. This is a hot
    // path (`oxy-customer-apps-perf`) — the loader costs ~5-7 queries (org sets,
    // platform standing, partner standings, app_members, org_team_members,
    // app_team_grants), and in the overwhelmingly common workspace where nothing is
    // restricted the filter below returns `true` without ever reading them. Behavior
    // is identical either way; this just stops every launcher render from paying for
    // a decision it never makes.
    //
    // Scoped facts: no app ring reads the workspace override, so don't pay for it.
    // Unknown facts (a DB blip) and an absent viewer both fail CLOSED here, unlike
    // the access gates — a missing card is a recoverable annoyance, a leaked one is
    // not, and this is discovery rather than an access decision so a wrong deny
    // costs nobody their app. Unrestricted apps are unaffected either way, so the
    // degraded case is exactly today's behavior.
    let facts = match viewer.filter(|_| any_restricted) {
        Some(v) => {
            oxy_server_authz::loader::load_principal_facts_scoped(db, v.id, v.email, false).await
        }
        None => None,
    };
    let rows: Vec<apps::Model> = rows
        .into_iter()
        .filter(|app| {
            if !app.is_restricted() {
                return true;
            }
            facts.as_ref().is_some_and(|f| {
                oxy_authz::allows(
                    f,
                    oxy_authz::Action::AppAccess,
                    &oxy_authz::Resource::app_with_visibility(app.id, app.org_id, true),
                )
            })
        })
        .collect();

    if rows.is_empty() {
        return Ok(vec![]);
    }

    // Bulk-resolve org slugs in one query (apps for a workspace will
    // usually share one org, but the field is denormalised so we don't
    // assume).
    use std::collections::{HashMap, HashSet};
    let org_ids: HashSet<Uuid> = rows.iter().map(|a| a.org_id).collect();
    let orgs = Organizations::find()
        .filter(organizations::Column::Id.is_in(org_ids.iter().copied()))
        .all(db)
        .await?;
    let slugs: HashMap<Uuid, String> = orgs.into_iter().map(|o| (o.id, o.slug)).collect();

    // Card metadata for the whole page in ONE batched `app_builds` query (no
    // N+1) — the same resolver the admin list uses. See the
    // `oxy-app-visual-identity` skill.
    let manifests =
        crate::server::api::custom_apps_manifest::resolve_manifests_batch(db, &rows).await;
    let mut out = Vec::with_capacity(rows.len());
    for app in rows {
        let Some(slug) = slugs.get(&app.org_id).cloned() else {
            continue;
        };
        // SAFETY-ish: we filtered on PublishedAt.is_not_null above.
        let Some(published) = app.published_at.map(|d| d.to_rfc3339()) else {
            continue;
        };
        let manifest = manifests.get(&app.id);
        out.push(CustomAppSummary::from_model_with_manifest(
            app, &slug, published, manifest,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod summary_mapping_tests {
    use super::*;

    #[test]
    fn missing_manifest_yields_empty_card_fields() {
        let f = manifest_card_fields(None);
        assert!(f.description.is_none());
        assert!(f.default_agent.is_none());
        assert!(f.suggested_questions.is_empty());
        assert!(f.art.is_none());
        assert!(f.icon.is_none());
        assert!(f.status.is_none());
    }

    #[test]
    fn manifest_card_fields_map_through() {
        let m: OxyAppManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "slug": "x",
            "description": "d",
            "art": "shots/card.png",
            "icon": "icon.svg",
            "status": "23 stores · live",
            "ask": { "agent": "a.yml", "suggestedQuestions": ["q1", "q2"] }
        }))
        .unwrap();
        let f = manifest_card_fields(Some(&m));
        assert_eq!(f.description.as_deref(), Some("d"));
        assert_eq!(f.default_agent.as_deref(), Some("a.yml"));
        assert_eq!(f.suggested_questions, vec!["q1", "q2"]);
        assert_eq!(f.art.as_deref(), Some("shots/card.png"));
        assert_eq!(f.icon.as_deref(), Some("icon.svg"));
        assert_eq!(f.status.as_deref(), Some("23 stores · live"));
    }

    #[test]
    fn unsafe_icon_path_is_dropped_but_other_fields_survive() {
        let m: OxyAppManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "slug": "x",
            "art": "card.png",
            "icon": "../escape.svg"
        }))
        .unwrap();
        let f = manifest_card_fields(Some(&m));
        assert_eq!(f.art.as_deref(), Some("card.png"));
        assert!(f.icon.is_none());
    }

    #[test]
    fn unsafe_art_paths_are_rejected() {
        let cases: &[(&str, bool)] = &[
            ("../x.png", false),
            ("/abs.png", false),
            ("https://evil/x.png", false),
            ("a\\b.png", false),
            ("card.png?v=1", false),
            ("card.png#anchor", false),
            ("card.png", true),
            ("shots/card.png", true),
        ];
        for (path, expect_some) in cases {
            let result = safe_relative_art_path(path);
            assert_eq!(
                result, *expect_some,
                "safe_relative_art_path({path:?}) should be {expect_some}"
            );
        }
    }
}
