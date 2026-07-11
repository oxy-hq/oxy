//! Cloud-only global routes: logout, organization CRUD (with org-scoped
//! sub-routes for members, invitations, onboarding, workspaces, GitHub
//! integration) and the per-user GitHub account/installation routes.
//!
//! Not mounted in local mode — see [`super::protected`].

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, patch, post, put};

use crate::api::billing;
use crate::api::github::namespaces as github;
use crate::api::github::{account, callback, installations};
use crate::api::middlewares::{
    org_context, oxy_app_admin_guard, oxy_owner_or_app_admin_guard, subscription_guard,
};
use crate::api::{admin, onboarding, org_logo, organizations, user, workspaces};

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
        // `/admin/*` runs under the permissive owner-or-app-admin guard so
        // app admins can reach feature flags, customer apps, orgs / users /
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
            admin::internal_jobs::router().layer(middleware::from_fn(
                oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
            )),
        )
        // Compile boundary operator surface (Phase 1.6a+). List recent
        // revisions, drill into one, manually enqueue a Compile task.
        // Same guard layering as internal-jobs.
        .nest(
            "/admin/compiles",
            admin::compiles::router().layer(middleware::from_fn(
                oxy_owner_or_app_admin_guard::oxy_owner_or_app_admin_guard_middleware,
            )),
        )
        // Parallel customer-apps surface for OXY_GLOBAL_ADMINS. Reuses the same
        // handlers as /admin/apps but gated by a separate role so app admins
        // can manage customer-app registrations without org/billing access.
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
                // Activity (usage tracking) — see `customer_apps_activity`.
                // Reads the `custom_app_view_event` + `custom_app_event`
                // tables to power the AppDetail "Activity" tab.
                .route(
                    "/{id}/activity/summary",
                    get(crate::server::api::customer_apps_activity::get_summary),
                )
                .route(
                    "/{id}/activity/visitors",
                    get(crate::server::api::customer_apps_activity::get_visitors),
                )
                .route(
                    "/{id}/activity/events",
                    get(crate::server::api::customer_apps_activity::get_events),
                )
                // Preview-draft cookie: flips this staff session into
                // draft view on the customer URL. Replaces the
                // (discoverable) `?view=draft` query param so the
                // customer URL surface stays free of any "press here
                // to flip" affordance. See `customer_apps_preview`.
                .route(
                    "/preview-draft",
                    post(crate::server::api::customer_apps_preview::enable_preview_draft)
                        .delete(crate::server::api::customer_apps_preview::disable_preview_draft),
                )
                // Server-side folder picker for the "Add customer app"
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
                // Mint an app-scoped API key. The plaintext key is
                // returned **once** for the operator to paste into the
                // bundle's deploy env (e.g. `OXY_API_KEY` in Vercel). The
                // Vercel-hosted Next.js server-side calls use it to
                // authenticate back into oxy. See
                // `customer_apps_api_keys`.
                .route(
                    "/{id}/api-keys",
                    post(crate::server::api::customer_apps_api_keys::mint),
                )
                // One-way publish entry point: CI (or local `oxy publish`)
                // uploads a built bundle tarball; oxy stores it in S3 and
                // points the draft (or published, with --promote) channel
                // at the new build. Raised body limit — bundles are a few
                // MB, well over axum's 2 MB default. See
                // `customer_apps_publish`.
                .route(
                    "/publish",
                    post(crate::server::api::customer_apps_publish::publish_handler)
                        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
                )
                .layer(middleware::from_fn(
                    oxy_app_admin_guard::oxy_app_admin_guard_middleware,
                )),
        )
        .nest("/user/github", build_user_github_routes())
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
        .route("/members", get(organizations::list_members))
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
        .nest("/github", build_github_routes())
        // Slack installation management (requires org membership, admin check inside handlers)
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

fn build_github_routes() -> Router<AppState> {
    Router::new()
        .route("/repositories", get(github::list_repositories))
        .route("/branches", get(github::list_branches))
        .route("/namespaces", get(github::list_git_namespaces))
        .route("/namespaces/pat", post(github::create_pat_namespace))
        .route(
            "/namespaces/installation",
            post(github::create_installation_namespace),
        )
        .route("/namespaces/{id}", delete(github::delete_git_namespace))
}

fn build_user_github_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/account",
            get(account::get_account).delete(account::delete_account),
        )
        .route("/account/oauth-url", get(account::get_oauth_url))
        .route("/installations", get(installations::list_installations))
        .route(
            "/installations/new-url",
            get(installations::get_new_installation_url),
        )
        .route("/callback", post(callback::callback))
}
