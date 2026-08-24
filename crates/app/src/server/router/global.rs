//! Cloud-only global routes: logout, organization CRUD (with org-scoped
//! sub-routes for members, invitations, onboarding, workspaces, GitHub
//! integration) and the per-user GitHub account/installation routes.
//!
//! Not mounted in local mode — see [`super::protected`].

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};

use crate::api::billing;
use crate::api::middlewares::{
    app_scope_guard, org_context, oxy_owner_or_app_admin_guard, platform_cap_guard,
    subscription_guard,
};
use crate::api::{admin, onboarding, org_logo, org_teams, organizations, user, workspaces};

use super::AppState;

pub(super) fn build_global_routes() -> Router<AppState> {
    Router::new()
        .route("/logout", get(user::logout))
        .route("/orgs", post(organizations::create_org))
        .route("/orgs", get(organizations::list_orgs))
        .route(
            "/apps/mine",
            get(crate::server::api::admin::apps::handlers::list_my_apps),
        )
        .route("/invitations/mine", get(organizations::list_my_invitations))
        .route(
            "/invitations/{token}/accept",
            post(organizations::accept_invitation),
        )
        .merge(airhouse::api::router::<AppState>())
        .nest("/orgs/{org_id}", build_org_routes())
        // Assume-role ("act as"). Authenticated, NOT admin-gated: staff act as any
        // org, a partner acts as an assigned client with `develop_apps`. The
        // handlers authorize (`assume::may_act_as`). It must sit outside /admin
        // because acting CLOSES /admin — the exit cannot live behind the door it
        // locks.
        .merge(admin::assume::router())
        // `/admin/*` runs under the permissive owner-or-app-admin guard so
        // app admins can reach feature flags, custom apps, orgs / users /
        // workspaces management, and internal jobs. The sensitive subset —
        // billing operations and the `app_admins` table itself — escalates
        // to strict OXY_OWNER via `route_layer` inside `admin::router()`.
        .nest(
            "/admin",
            admin::router().layer(middleware::from_fn(
                oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
            )),
        )
        // Internal Jobs is mounted as a sibling nest because its routes
        // were flattened (no `/internal-jobs/` prefix on each route). The
        // outer guard mirrors the broader `/admin/*` permissive guard.
        .nest(
            "/admin/internal-jobs",
            admin::internal_jobs::router()
                // Operating the durable task fleet is Oxy's own machinery, not tenant
                // data — `Cap::OperatePlatform`. A sibling nest gets no capability gate
                // from `admin::router`, so it names its own, exactly as it must name
                // its own `block_admin_while_acting`.
                .layer(middleware::from_fn(platform_cap_guard::require(
                    crate::server::authz::Action::PlatformOperate,
                )))
                // Refuse the staff surface while acting, exactly like `admin::router`
                // does internally. This is a SIBLING nest — it never passes through
                // `admin::router`, so the block it applies does not reach here. Any
                // future `/admin/*` sibling needs this line too, or it silently
                // becomes drivable mid-impersonation.
                .layer(middleware::from_fn(admin::assume::block_admin_while_acting))
                .layer(middleware::from_fn(
                    oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
                )),
        )
        // Compile boundary operator surface (Phase 1.6a+). List recent
        // revisions, drill into one, manually enqueue a Compile task.
        // Same guard layering as internal-jobs.
        .nest(
            "/admin/compiles",
            admin::compiles::router()
                // Compile history is platform machinery — `Cap::OperatePlatform`.
                .layer(middleware::from_fn(platform_cap_guard::require(
                    crate::server::authz::Action::PlatformOperate,
                )))
                // Sibling nest — see internal-jobs above.
                .layer(middleware::from_fn(admin::assume::block_admin_while_acting))
                .layer(middleware::from_fn(
                    oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
                )),
        )
        // Parallel customer-apps surface for OXY_GLOBAL_ADMINS. Reuses the same
        // handlers as /admin/apps but gated by a separate role so app admins
        // can manage custom-app registrations without org/billing access.
        .nest(
            "/customer-apps",
            Router::new()
                .route(
                    "/",
                    post(admin::apps::handlers::create_app).get(admin::apps::handlers::list_apps),
                )
                .route(
                    "/{id}",
                    get(admin::apps::handlers::get_app)
                        .patch(admin::apps::handlers::update_app)
                        .delete(admin::apps::handlers::delete_app),
                )
                // Publish / unpublish lives on the app-admin surface too —
                // shipping an app to the customer is a normal Oxy-engineer
                // workflow, not an OXY_OWNER-only action.
                .route(
                    "/{id}/publish",
                    post(admin::apps::handlers::publish_app)
                        .delete(admin::apps::handlers::unpublish_app),
                )
                // Oxy Functions management/debug surface (read): list functions
                // + config, one function's invocation history, and a job run's
                // status + logs. Same handlers as the /admin/apps surface.
                .route(
                    "/{id}/functions",
                    get(admin::apps::functions::list_functions),
                )
                .route(
                    "/{id}/functions/{name}/invocations",
                    get(admin::apps::functions::list_invocations),
                )
                .route(
                    "/{id}/function-runs/{run_id}",
                    get(admin::apps::functions::get_function_run),
                )
                // Manually trigger a one-off background run of an app function
                // as a job (the "run now" not tied to a cron schedule). Same
                // handler as the /admin/apps surface. See the Function Jobs doc.
                .route(
                    "/{id}/functions/{name}/runs",
                    post(admin::apps::handlers::run_function_job),
                )
                // New-pipeline build lifecycle: list versioned builds and
                // roll the published channel back to any retained one.
                // Pointer moves only — bytes already live in S3.
                .route("/{id}/builds", get(admin::apps::handlers::list_builds))
                .route("/{id}/rollback", post(admin::apps::handlers::rollback_app))
                // Batch mutations for the admin apps table: publish /
                // unpublish / delete many apps in one request. POST even for
                // delete, since the id set travels in the body. Each is
                // best-effort per-id (see `BatchResponse`); `batch` is a
                // static segment so it never collides with `/{id}`.
                .route(
                    "/batch/publish",
                    post(admin::apps::handlers::batch_publish_apps),
                )
                .route(
                    "/batch/promote-latest",
                    post(admin::apps::handlers::batch_promote_latest_apps),
                )
                .route(
                    "/batch/unpublish",
                    post(admin::apps::handlers::batch_unpublish_apps),
                )
                .route(
                    "/batch/delete",
                    post(admin::apps::handlers::batch_delete_apps),
                )
                // Activity (usage tracking) — see `custom_apps_activity`.
                // Reads the `custom_app_view_event` + `custom_app_event`
                // tables to power the AppDetail "Activity" tab.
                .route(
                    "/{id}/activity/summary",
                    get(crate::server::api::custom_apps_activity::get_summary),
                )
                .route(
                    "/{id}/activity/visitors",
                    get(crate::server::api::custom_apps_activity::get_visitors),
                )
                .route(
                    "/{id}/activity/events",
                    get(crate::server::api::custom_apps_activity::get_events),
                )
                // Preview-draft cookie: flips this staff session into
                // draft view on the customer URL. Replaces the
                // (discoverable) `?view=draft` query param so the
                // customer URL surface stays free of any "press here
                // to flip" affordance. See `custom_apps_preview`.
                .route(
                    "/preview-draft",
                    post(crate::server::api::custom_apps_preview::enable_preview_draft)
                        .delete(crate::server::api::custom_apps_preview::disable_preview_draft),
                )
                // Server-side folder picker for the "Add custom app"
                // dialog's local-link / create-new flow. Local-mode only;
                // returns 404 in cloud. See `admin::apps::fs`.
                .route("/fs/listdir", get(admin::apps::fs::listdir))
                // Bundle identity probe: reads oxy-app.json + index.html
                // for the picked folder so the dialog can lock name/slug
                // to what the bundle declares.
                .route("/fs/probe", get(admin::apps::fs::probe))
                // Template gallery for the Create-new dialog. No
                // screenshot route yet — re-add when the first PNG
                // ships (see templates.rs module docstring).
                .route("/templates", get(admin::apps::templates::list_templates))
                // Org / project browser: every workspace that granted Oxy
                // access, flattened with its org + grant metadata. See
                // `admin::oxy_access`.
                .route("/oxy-access", get(admin::oxy_access::list_grants))
                // ── Custom-app storage (see the asset-lifecycle design doc) ──
                // Fleet view: every measured app ranked by size / 7d growth /
                // untagged bytes. Reads the `app_storage_usage` rollup, never S3
                // — ranking the fleet cannot mean walking every silo per request.
                .route("/storage", get(admin::apps::storage::fleet))
                // Force a re-measure. Without this the fleet view's staleness is
                // visible but unactionable.
                .route("/storage/sweep", post(admin::apps::storage::sweep_now))
                // Daily totals for the usage-over-time chart. Fleet-wide by
                // default; `?appId=` narrows it to one app.
                .route("/storage/history", get(admin::apps::storage::history))
                // Month-to-date GB-month for one org (time-weighted).
                .route(
                    "/storage/meter/{org_id}",
                    get(admin::apps::storage::org_meter),
                )
                // Per-app browser: reads S3 live via `list()`, because the rollup
                // holds no per-object rows and an operator investigating now
                // needs current truth, not a number a sweep old.
                .route("/{id}/storage/objects", get(admin::apps::storage::browse))
                .route(
                    "/{id}/storage/delete",
                    post(admin::apps::storage::delete_objects),
                )
                // Mint an app-scoped API key. The plaintext key is
                // returned **once** for the operator to paste into the
                // bundle's deploy env (e.g. `OXY_API_KEY` in Vercel). The
                // Vercel-hosted Next.js server-side calls use it to
                // authenticate back into oxy. See
                // `custom_apps_api_keys`.
                .route(
                    "/{id}/api-keys",
                    post(crate::server::api::custom_apps_api_keys::mint),
                )
                // Trusted-publishing config: register / list / remove the GitHub
                // workflows allowed to OIDC-publish this app. See
                // `custom_apps_publish_oidc`.
                .route(
                    "/{id}/publishers",
                    get(crate::server::api::custom_apps_publish_oidc::list_publishers)
                        .post(crate::server::api::custom_apps_publish_oidc::register_publisher),
                )
                .route(
                    "/{id}/publishers/{publisher_id}",
                    axum::routing::delete(
                        crate::server::api::custom_apps_publish_oidc::delete_publisher,
                    ),
                )
                // Everything ABOVE is the interactive customer-apps admin surface
                // (create / update / delete / rollback / batch): refuse it while a
                // staff operator is acting as a tenant, closing the gap that this
                // sibling nest never passed through `admin::router`'s block. `axum`
                // applies a `.layer` only to routes registered before it, so this
                // covers the routes above and NOT `/publish` below.
                .layer(middleware::from_fn(admin::assume::block_admin_while_acting))
                // Scope: which apps may this grant touch. Layered over the whole tree so
                // the ~20 `/{id}` routes above don't each have to remember — see
                // `app_scope_guard`. Same `.layer` ordering rule: covers the routes
                // registered before it, not `/publish` (which resolves its own actor via
                // `custom_apps_publish_authz::resolve_actor`, scope included).
                .layer(middleware::from_fn(app_scope_guard::enforce_app_scope))
                // The custom-app lifecycle IS `Cap::ManageApps` — this surface is the
                // one an App Operator exists to use, so it is gated on the capability
                // rather than on being staff. Same `.layer` ordering rule as above:
                // applies to the routes registered before it, not to `/publish`.
                .layer(middleware::from_fn(platform_cap_guard::require(
                    crate::server::authz::Action::PlatformApps,
                )))
                // Owner-or-admin, not admin-only. A Global Owner who isn't also in
                // `app_admins` was 403'd here — locking the MORE senior role out of a
                // surface the junior one runs. Both tiers reach the custom-app
                // lifecycle; only owner-exclusive destructive operations separate them.
                .layer(middleware::from_fn(
                    oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
                ))
                // One-way publish entry point: CI (or local `oxy publish`) uploads
                // a built bundle tarball. Registered AFTER both layers above, so
                // it is neither app-admin-gated nor blocked-while-acting:
                //   * NOT app-admin-gated — authorization is decided INSIDE
                //     `publish()` by the three gates (org officer, or a partner
                //     with manage_apps + assignment + the client's consent, or
                //     staff-unless-locked). Gating the route to app_admins would
                //     have made partner publish impossible.
                //   * NOT blocked-while-acting — it is also the CI endpoint reached
                //     by a publish token; blocking it would 403 a CI job merely
                //     because the token's minter has a browser assume-session open.
                // It remains behind the outer auth + `app_publish_token_scope`
                // layers, so it is always authenticated. Raised body limit —
                // bundles are a few MB, over axum's 2 MB default.
                .route(
                    "/publish",
                    post(crate::server::api::custom_apps_publish::publish_handler)
                        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
                ),
        )
    // NOTE: Slack webhook + OAuth-callback + magic-link routes are NOT
    // registered here. They must live in `public.rs` because the routes
    // in this file sit inside the auth middleware layer, and:
    //   - Slack's webhook POSTs (/slack/events, /slack/interactivity) have
    //     no user auth — they're signature-verified inside the handler.
    //   - The OAuth callback is reached via a browser redirect from
    //     slack.com — the browser carries no Authorization header.
    //   - The magic-link landing handles auth state itself via
    //     OptionalAuthenticatedUser; bouncing through the auth middleware
    //     first would 401 every unauth'd user.
}

fn build_org_routes() -> Router<AppState> {
    // Two sub-routers under the same `org_middleware`:
    //   - `gated` covers everything that requires an active subscription
    //     (members, invitations, onboarding, workspace CRUD, github)
    //   - `bypass` covers `/billing/*`, the only org-scoped tree the user
    //     can hit while paywalled (so they can subscribe / open the portal)
    let gated = Router::new()
        .route("/", get(organizations::get_org))
        .route("/", patch(organizations::update_org))
        .route("/", delete(organizations::delete_org))
        .route(
            "/logo",
            put(org_logo::upload_org_logo).delete(org_logo::delete_org_logo),
        )
        .route(
            "/partner-publish-consent",
            get(crate::server::api::partner_publish_consent::get_consent)
                .put(crate::server::api::partner_publish_consent::set_consent),
        )
        .route("/members", get(organizations::list_members))
        // Teams + per-app access — the control plane for restricted custom apps.
        // Pure Postgres (no FS, no git), so every route here is FleetOk.
        .route(
            "/teams",
            get(org_teams::handlers::list_teams).post(org_teams::handlers::create_team),
        )
        .route(
            "/teams/{team_id}",
            get(org_teams::handlers::get_team)
                .patch(org_teams::handlers::update_team)
                .delete(org_teams::handlers::delete_team),
        )
        .route(
            "/teams/{team_id}/members",
            post(org_teams::handlers::add_team_member),
        )
        .route(
            "/teams/{team_id}/members/{user_id}",
            delete(org_teams::handlers::remove_team_member),
        )
        .route("/apps", get(org_teams::app_access::list_org_apps))
        .route(
            "/apps/{app_id}/access",
            get(org_teams::app_access::get_app_access).put(org_teams::app_access::set_app_access),
        )
        .route(
            "/members/{user_id}",
            patch(organizations::update_member_role),
        )
        .route("/members/{user_id}", delete(organizations::remove_member))
        .route("/invitations", post(organizations::create_invitation))
        .route(
            "/invitations/bulk",
            post(organizations::create_bulk_invitations),
        )
        .route("/invitations", get(organizations::list_invitations))
        .route(
            "/invitations/{invitation_id}",
            delete(organizations::revoke_invitation),
        )
        .route("/onboarding/demo", post(onboarding::setup_demo))
        .route("/onboarding/new", post(onboarding::setup_new))
        .route("/onboarding/github", post(onboarding::setup_github))
        .route("/workspaces", get(workspaces::list_workspaces))
        .route("/workspaces/{id}", delete(workspaces::delete_workspace))
        .route(
            "/workspaces/{id}/rename",
            patch(workspaces::rename_workspace),
        )
        // Slack installation management. The admin check is the `OrgAdmin` extractor on the
        // handlers themselves, not a hand-rolled role match — `get_status` is member-level.
        .route(
            "/slack/install",
            post(crate::integrations::slack::oauth::install::start_install),
        )
        .route(
            "/slack/installation",
            get(crate::integrations::slack::oauth::status::get_status)
                .delete(crate::integrations::slack::oauth::disconnect::disconnect),
        )
        .layer(middleware::from_fn(
            subscription_guard::subscription_guard_middleware,
        ));

    let bypass = Router::new().nest("/billing", billing::router());

    gated
        .merge(bypass)
        .layer(middleware::from_fn(org_context::org_middleware))
}
