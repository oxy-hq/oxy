//! `oxy apps` — manage custom-app registrations + scaffold new bundles.
//!
//! Two flavors of subcommand share the namespace because they're both
//! "things you do to custom apps from the CLI":
//!
//! - **Registration** (`create`, `list`, `delete`) — thin wrappers
//!   around the admin endpoint handlers. Calls them directly without
//!   needing an HTTP server. Useful for ops + CI scripts.
//! - **Bundle authoring** (`init`) — generates a new custom-app
//!   bundle from a baked-in template. Lives next to registration so
//!   the natural CLI flow is `oxy apps init my-app` → write code →
//!   commit + push to the customer-apps repo (CI runs build + S3
//!   upload + `oxy apps ensure` from `deploy.yml.example`).
//!
//! Template files ship inside the binary via `include_dir!` — same
//! pattern as `demo_project` in `oxy init`.

use axum::{
    Json,
    extract::{Path, Query},
};
use clap::Parser;
use entity::organizations;
use entity::prelude::Organizations;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::server::api::admin::apps::handlers::{self, CreateAppRequest};
use crate::server::api::custom_apps_source::SourceSpec;

#[derive(Parser, Debug)]
pub struct AppsArgs {
    #[clap(subcommand)]
    pub command: AppsCommand,
}

#[derive(Parser, Debug)]
pub enum AppsCommand {
    /// Look up an app by `(org, slug)` and print the row.
    Show {
        /// Org slug.
        #[clap(long)]
        org: String,
        /// App slug.
        #[clap(long)]
        slug: String,
    },
    /// Register a new custom app
    Create {
        /// Human-readable display name
        #[clap(long)]
        name: String,
        /// Owning organization, identified by slug (preferred for humans) or
        /// by uuid. Exactly one must be provided.
        #[clap(long, conflicts_with = "org_id")]
        org_slug: Option<String>,
        #[clap(long, conflicts_with = "org_slug")]
        org_id: Option<Uuid>,
        /// UUID of the Oxy project this app belongs to
        #[clap(long)]
        project_id: Uuid,
        /// Git branch to track for deployments (default: main)
        #[clap(long, default_value = "main")]
        branch: String,
    },
    /// List all registered apps
    List,
    /// Delete a registered app by ID
    Delete {
        /// UUID of the app to delete
        #[clap(long)]
        id: Uuid,
    },
}

/// Dispatch `oxy apps …`.
pub async fn handle_apps_command(args: AppsArgs) -> Result<(), OxyError> {
    match args.command {
        AppsCommand::Show { org, slug } => handle_show(org, slug).await,
        AppsCommand::Create {
            name,
            org_slug,
            org_id,
            project_id,
            branch,
        } => handle_create(name, org_slug, org_id, project_id, branch).await,
        AppsCommand::List => handle_list().await,
        AppsCommand::Delete { id } => handle_delete(id).await,
    }
}

// `validate_app_name` + `title_case` were retired with `oxy apps ensure`:
// `create_app` does its own name validation, and the publish pipeline's
// `humanize_slug` covers display-name derivation.

#[cfg(test)]
mod init_tests {
    use crate::custom_app_template::Substitutions;

    #[test]
    fn substitutions_replace_placeholders() {
        let sub = Substitutions {
            app_slug: "my-app",
            app_display_name: "My App",
            app_base_path: "/customer-apps/acme/my-app/",
        };
        let out = sub
            .apply("name: {{APP_SLUG}}\ntitle: {{APP_DISPLAY_NAME}}\nbase: {{OXY_APP_BASE_PATH}}");
        assert_eq!(
            out,
            "name: my-app\ntitle: My App\nbase: /customer-apps/acme/my-app/"
        );
    }
}

// ── `oxy apps create` ───────────────────────────────────────────────────────

async fn handle_create(
    name: String,
    org_slug: Option<String>,
    org_id: Option<Uuid>,
    project_id: Uuid,
    branch: String,
) -> Result<(), OxyError> {
    let org_id = match (org_id, org_slug) {
        (Some(id), _) => id,
        (None, Some(slug)) => resolve_org_id_by_slug(&slug).await?,
        (None, None) => {
            return Err(OxyError::RuntimeError(
                "Provide either --org-slug or --org-id".to_string(),
            ));
        }
    };

    let req = CreateAppRequest {
        name,
        org_id,
        project_id,
        branch,
        // CLI defaults: no caller-supplied slug (handler auto-derives) and
        // no scaffold PR (CI is the canonical scaffold path; CLI is for
        // ops scripting where the row should land without side effects).
        slug: None,
        source: SourceSpec::S3,
        scaffold_pr: false,
        provision_local_source: false,
        // CLI callers get the default "vite" template; a dedicated
        // --template flag can be wired up in a follow-up if needed.
        template_id: None,
        // CLI create lets the handler default `repo_path` to the
        // `<org_slug>/<slug>` pair — matches the CI matrix shape and
        // keeps the S3 path stable across envs that run this command
        // with the same args.
        repo_path: None,
    };
    let resp = handlers::create_app(Json(req))
        .await
        .map_err(|(sc, body)| {
            OxyError::RuntimeError(format!(
                "create_app failed with status {sc}: {}",
                body.0.message
            ))
        })?;

    let json = serde_json::to_string_pretty(&resp.0)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize response: {e}")))?;
    println!("{}", json);
    println!();
    println!("{}", "Next steps:".text());
    println!("  1. cd customer-apps    (clone oxy-hq/customer-apps if you haven't)");
    println!("  2. Create directory apps/{}/", resp.0.id);
    println!("  3. Scaffold app code (any v0/Next.js source) into that directory");
    println!("  4. git push origin main to trigger CI deploy");
    println!("  5. App will be live at {}", resp.0.url.secondary());
    Ok(())
}

async fn handle_list() -> Result<(), OxyError> {
    let resp = handlers::list_apps(Query(handlers::ListAppsQuery::default()))
        .await
        .map_err(|sc| OxyError::RuntimeError(format!("list_apps failed with status {sc}")))?;

    let json = serde_json::to_string_pretty(&resp.0)
        .map_err(|e| OxyError::RuntimeError(format!("Failed to serialize response: {e}")))?;
    println!("{}", json);
    Ok(())
}

async fn handle_delete(id: Uuid) -> Result<(), OxyError> {
    handlers::delete_app(Path(id))
        .await
        .map_err(|sc| OxyError::RuntimeError(format!("delete_app failed with status {sc}")))?;
    println!("{}", format!("Deleted app {id}").success());
    Ok(())
}

async fn resolve_org_id_by_slug(slug: &str) -> Result<Uuid, OxyError> {
    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("DB connect failed: {e}")))?;
    let org = Organizations::find()
        .filter(organizations::Column::Slug.eq(slug))
        .one(&db)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Query org failed: {e}")))?
        .ok_or_else(|| OxyError::RuntimeError(format!("No org with slug {slug}")))?;
    Ok(org.id)
}

// ── `oxy apps show` ─────────────────────────────────────────────────────────
//
// Inspect an app by `(org_slug, app_slug)`. Operates directly on the apps
// table via SeaORM. (The CI upsert + sync commands were retired in favor of
// `oxy publish`, which the customer-apps CI now calls instead.)

async fn handle_show(org_slug: String, app_slug: String) -> Result<(), OxyError> {
    use entity::apps;
    use entity::prelude::Apps;

    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("DB connect failed: {e}")))?;
    let org_id = resolve_org_id_by_slug(&org_slug).await?;

    let row = Apps::find()
        .filter(apps::Column::OrgId.eq(org_id))
        .filter(apps::Column::Slug.eq(&app_slug))
        .one(&db)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("Query app failed: {e}")))?
        .ok_or_else(|| {
            OxyError::RuntimeError(format!("No app '{app_slug}' in org '{org_slug}'"))
        })?;

    let json = serde_json::to_string_pretty(&row)
        .map_err(|e| OxyError::RuntimeError(format!("Serialize app failed: {e}")))?;
    println!("{json}");
    Ok(())
}
