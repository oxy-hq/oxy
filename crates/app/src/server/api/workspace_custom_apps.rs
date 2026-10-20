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
use axum::extract::{Path, Query};
use axum::http::StatusCode;
use entity::organizations;
use entity::prelude::{Apps, Organizations};
use entity::{apps, apps::Model as AppsModel};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::OptionalUserExtractor;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::admin::apps::verdict::{AppRef, ListVerdict, verdicts_for};
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
    /// Is this app OK, per the server: `AppHealth`'s values plus `draft`, from the
    /// same evaluator as `/customer-apps/fleet-health` (`admin::apps::verdict`).
    /// Unlike [`status`](Self::status) — free text the app's author writes — this
    /// is measured.
    ///
    /// Filled by [`list_custom_apps`] **only when the caller asks** with
    /// `?verdict=true` — the launcher's health signal does. The workspace shell
    /// fetches this same endpoint on every page for its rail, and the shell-context
    /// and `/me/apps` lists share these summaries: none of them may wait on an
    /// observability read. Absent means "not asked" or "could not say", never
    /// healthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<ListVerdict>,
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
/// never fails on a metadata problem. Plain display data only — `icon`/`art`
/// become URLs through [`icon_art_urls`], the one builder every surface uses.
#[derive(Default)]
struct CardFields {
    description: Option<String>,
    default_agent: Option<String>,
    suggested_questions: Vec<String>,
    status: Option<String>,
}

/// Resolve a manifest's `icon`/`art` relative paths to same-origin URLs under
/// the app's canonical bundle URL, returning `(icon_url, art_url)`. Shared by
/// the homepage launcher list and the admin apps list so every surface shows
/// the same picture from the one manifest source. See the
/// `oxy-app-visual-identity` skill.
///
/// ## `?v=<production's build id>`
///
/// Icon and art are unfingerprinted bundle-root files, so a bare URL is only
/// ever `public, max-age=300` at the origin — a longer lifetime would show a
/// stale picture after a publish. Every launcher render past five minutes paid
/// a round trip and an auth check per image as a result. Naming the published
/// build in the URL gives each publish its own URL, which lets the serve route
/// (`custom_apps_serve::headers::cache_control_for`) answer `immutable` for it.
///
/// `v` is the `app_builds` **primary key** (production's `app_environments.build_id`), not the
/// engineer-facing `app_builds.build_id` string: the pk is what a promote or
/// rollback repoints, and it is what the serve route compares `v` against.
///
/// `None` — never published — emits the bare URL exactly as before; there is no
/// build to pin, so the origin keeps its revalidating policy. The `?` cannot
/// collide with a query already in the path: `safe_relative_art_path` rejects
/// `?` and `#`.
pub(super) fn icon_art_urls(
    manifest: Option<&OxyAppManifest>,
    org_slug: &str,
    app_slug: &str,
    production_build: Option<Uuid>,
) -> (Option<String>, Option<String>) {
    let Some(m) = manifest else {
        return (None, None);
    };
    // Trailing slash matches `build_pretty_url`, so the relative path appends
    // to a valid same-origin URL.
    let base = build_pretty_url(org_slug, app_slug);
    let version = production_build
        .map(|id| format!("?v={id}"))
        .unwrap_or_default();
    let to_url = |p: &str| format!("{base}{p}{version}");
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
        status: m.status.clone(),
    }
}

impl CustomAppSummary {
    fn from_model_with_manifest(
        m: AppsModel,
        org_slug: &str,
        published_at: String,
        production_build: Option<Uuid>,
        manifest: Option<&OxyAppManifest>,
    ) -> Self {
        let url = build_pretty_url(org_slug, &m.slug);
        let fields = manifest_card_fields(manifest);
        // Through the shared builder rather than a local `format!`: this list
        // used to assemble the same URLs by hand, and a copy is exactly where
        // the `?v=` build pin would have been forgotten.
        let (icon_url, art_url) = icon_art_urls(manifest, org_slug, &m.slug, production_build);
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
            verdict: None,
        }
    }
}

#[derive(Deserialize, Default)]
pub struct ListCustomAppsQuery {
    /// Also compute each card's `verdict` (an observability read). Off by default:
    /// this endpoint feeds the workspace rail on every page.
    #[serde(default)]
    verdict: bool,
}

pub async fn list_custom_apps(
    OptionalUserExtractor(viewer): OptionalUserExtractor,
    Path(WorkspaceIdPath { workspace_id }): Path<WorkspaceIdPath>,
    Query(query): Query<ListCustomAppsQuery>,
) -> Result<Json<Vec<CustomAppSummary>>, StatusCode> {
    let db = establish_connection().await.map_err(|e| {
        tracing::error!("list_custom_apps DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    // Optional rather than required: the route is mounted behind auth middleware, so
    // a missing user means the middleware was bypassed — and the fail-closed filter
    // below is a better answer to that than a 401 that hides the misconfiguration.
    let viewer = viewer.as_ref().map(oxy_server_authz::Caller::from_user);
    let mut out = published_app_summaries(&db, workspace_id, viewer.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("list_custom_apps failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if query.verdict {
        attach_verdicts(&db, &mut out).await;
    }
    Ok(Json(out))
}

/// Fill each card's `verdict` from the fleet-health evaluator. Best-effort: a
/// failed lookup leaves the cards without one rather than failing the launcher.
async fn attach_verdicts(db: &sea_orm::DatabaseConnection, summaries: &mut [CustomAppSummary]) {
    use sea_orm::QuerySelect;
    if summaries.is_empty() {
        return;
    }
    // The card carries no org id, and the evaluator keys on `(org_id, app_id)`.
    let ids: Vec<Uuid> = summaries.iter().map(|s| s.id).collect();
    let orgs: Vec<(Uuid, Uuid)> = match Apps::find()
        .select_only()
        .column(apps::Column::Id)
        .column(apps::Column::OrgId)
        .filter(apps::Column::Id.is_in(ids))
        .into_tuple()
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("launcher verdicts: org lookup failed (no verdicts shown): {e}");
            return;
        }
    };
    // Every card here is a published app: `published_app_summaries` drops the rest.
    let refs: Vec<AppRef> = orgs
        .into_iter()
        .map(|(id, org_id)| AppRef {
            id,
            org_id,
            published: true,
        })
        .collect();
    let verdicts = verdicts_for(&refs).await;
    for summary in summaries {
        summary.verdict = verdicts.get(&summary.id).copied();
    }
}

/// Who is asking, for the visibility filter in [`published_app_summaries`]:
/// the user **with the credential the request arrived with**, so an API token's
/// grants and standing flags narrow the list as they narrow every other answer.
///
/// `None` at the call site means "nobody authenticated", which drops every
/// restricted app rather than 401ing — see the handler above.
pub type Viewer<'a> = &'a oxy_server_authz::Caller;

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
        Some(v) => oxy_server_authz::loader::load_principal_facts_scoped(db, v, false).await,
        None => None,
    };
    let rows: Vec<apps::Model> = rows
        .into_iter()
        .filter(|app| {
            if !app.is_restricted() {
                return true;
            }
            facts.as_ref().is_some_and(|f| {
                oxy_authz::allows(f, oxy_authz::Action::AppAccess, &app_access_resource(app))
            })
        })
        .collect();

    Ok(launcher_apps(db, rows)
        .await?
        .into_iter()
        .map(|app| app.summary)
        .collect())
}

/// The `AppAccess` resource for an app row: its id, org and visibility. One
/// builder for every list that asks "may this person open this app?", so none
/// of them can hand the ring a workspace id or a stale visibility.
pub(crate) fn app_access_resource(app: &apps::Model) -> oxy_authz::Resource {
    oxy_authz::Resource::app_with_visibility(app.id, app.org_id, app.is_restricted())
}

/// A published app as a launcher card, plus the org and workspace a list that
/// spans several of them needs to label and group it (`GET /api/me/apps`).
pub(crate) struct LauncherApp {
    pub org_id: Uuid,
    pub org_name: String,
    pub workspace_id: Uuid,
    pub summary: CustomAppSummary,
}

/// Cards for apps the caller has ALREADY been authorized to see — this filters
/// nothing for access. Two queries whatever the count: the orgs (slug + name,
/// not the logo blob) and the batched manifest read (see the
/// `oxy-app-visual-identity` skill). Input order is kept; a row whose org is
/// gone or that was never published is dropped.
pub(crate) async fn launcher_apps(
    db: &sea_orm::DatabaseConnection,
    rows: Vec<apps::Model>,
) -> Result<Vec<LauncherApp>, sea_orm::DbErr> {
    use sea_orm::QuerySelect;
    use std::collections::{HashMap, HashSet};
    if rows.is_empty() {
        return Ok(vec![]);
    }
    // The org is denormalised onto each app, so don't assume one org per page.
    let org_ids: HashSet<Uuid> = rows.iter().map(|a| a.org_id).collect();
    let orgs: HashMap<Uuid, (String, String)> = Organizations::find()
        .select_only()
        .column(organizations::Column::Id)
        .column(organizations::Column::Slug)
        .column(organizations::Column::Name)
        .filter(organizations::Column::Id.is_in(org_ids))
        .into_tuple::<(Uuid, String, String)>()
        .all(db)
        .await?
        .into_iter()
        .map(|(id, slug, name)| (id, (slug, name)))
        .collect();

    // One read of every card's environments: production's build pins the icon
    // and art URLs, and both builds resolve the manifests.
    let environments =
        crate::server::api::custom_apps_env_resolve::load_environment_builds_batch(db, &rows)
            .await?;
    let manifests =
        crate::server::api::custom_apps_manifest::resolve_manifests_batch(db, &rows, &environments)
            .await;
    let mut out = Vec::with_capacity(rows.len());
    for app in rows {
        let Some((org_slug, org_name)) = orgs.get(&app.org_id) else {
            continue;
        };
        let Some(published) = app.published_at.map(|d| d.to_rfc3339()) else {
            continue;
        };
        let (org_id, workspace_id) = (app.org_id, app.project_id);
        let manifest = manifests.get(&app.id);
        let production_build = environments.get(&app.id).and_then(|e| e.production);
        out.push(LauncherApp {
            org_id,
            org_name: org_name.clone(),
            workspace_id,
            summary: CustomAppSummary::from_model_with_manifest(
                app,
                org_slug,
                published,
                production_build,
                manifest,
            ),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod summary_mapping_tests {
    use super::*;

    fn manifest_with_images() -> OxyAppManifest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 2,
            "slug": "x",
            "icon": "icon.svg",
            "art": "shots/card.png"
        }))
        .unwrap()
    }

    #[test]
    fn missing_manifest_yields_empty_card_fields() {
        let f = manifest_card_fields(None);
        assert!(f.description.is_none());
        assert!(f.default_agent.is_none());
        assert!(f.suggested_questions.is_empty());
        assert!(f.status.is_none());
        assert_eq!(
            icon_art_urls(None, "acme", "ops", Some(Uuid::from_u128(1))),
            (None, None)
        );
    }

    /// Each publish gets its own image URLs, which is what lets the serve route
    /// call them `immutable` instead of revalidating every five minutes.
    #[test]
    fn icon_and_art_urls_carry_the_published_build() {
        let build = Uuid::from_u128(0x2a);
        let (icon, art) = icon_art_urls(Some(&manifest_with_images()), "acme", "ops", Some(build));
        assert_eq!(
            icon.as_deref(),
            Some(format!("/customer-apps/acme/ops/icon.svg?v={build}").as_str())
        );
        assert_eq!(
            art.as_deref(),
            Some(format!("/customer-apps/acme/ops/shots/card.png?v={build}").as_str())
        );
    }

    /// Never published: no build to pin, so the URL is exactly what it was
    /// before pinning existed — no `?v=`, not even an empty one.
    #[test]
    fn an_unpublished_app_gets_the_bare_urls() {
        let (icon, art) = icon_art_urls(Some(&manifest_with_images()), "acme", "ops", None);
        assert_eq!(icon.as_deref(), Some("/customer-apps/acme/ops/icon.svg"));
        assert_eq!(
            art.as_deref(),
            Some("/customer-apps/acme/ops/shots/card.png")
        );
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
        assert_eq!(f.status.as_deref(), Some("23 stores · live"));
        let (icon, art) = icon_art_urls(Some(&m), "acme", "ops", None);
        assert_eq!(icon.as_deref(), Some("/customer-apps/acme/ops/icon.svg"));
        assert_eq!(
            art.as_deref(),
            Some("/customer-apps/acme/ops/shots/card.png")
        );
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
        let (icon, art) = icon_art_urls(Some(&m), "acme", "ops", Some(Uuid::from_u128(3)));
        assert!(
            icon.is_none(),
            "an escaping path must not become a URL, pinned or not"
        );
        assert_eq!(
            art.as_deref(),
            Some(format!("/customer-apps/acme/ops/card.png?v={}", Uuid::from_u128(3)).as_str())
        );
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
