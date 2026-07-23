use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "apps")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// URL-safe identifier within the owning org. Unique on `(org_id, slug)`.
    /// Auto-derived from `name` on create; can be PATCH'd later.
    pub slug: String,
    pub name: String,
    pub org_id: Uuid,
    pub project_id: Uuid,
    pub branch: String,
    pub source_repo: String,
    pub status: String,
    /// Tagged source: `"v0"` | `"local"` | `"s3"`. Pairs with `source_config`
    /// to dispatch the bundle-serving handler. Default `"s3"` matches the
    /// pre-facade behavior so existing rows keep working.
    pub source_type: String,
    /// Variant payload. For `v0`: `{"url": "..."}`. For `local`:
    /// `{"path": "..."}`. For `s3`: `{}` (the bucket name + uuid prefix
    /// come from env vars). Stored as JSONB; deserialised at request time.
    pub source_config: Json,
    /// Set by `POST /api/customer-apps/<org>/<app>/sync` after a successful
    /// pull from S3. NULL means the app has never been synced.
    pub last_synced_at: Option<DateTimeWithTimeZone>,
    /// Per-deployment override of the bundle's bundled `oxy-app.json`.
    /// When set, the data-products endpoint serves THIS manifest
    /// instead of the file in the bundle dir — letting one bundle
    /// template back N customer deployments with different product
    /// configs (workspace IDs, dataset filters, etc.) without
    /// rebuilding. NULL = use the bundle's own manifest (the original
    /// behavior). Whole-replacement; no partial merge in v1 (the
    /// "which key wins" footguns aren't worth it yet). Wire shape
    /// matches `OxyAppManifest` from
    /// `crates/app/src/server/api/customer_apps_data_products.rs`.
    pub manifest_override: Option<Json>,
    /// Populated by the PR-scaffold service when `scaffold_pr: true` was
    /// passed on create. NULL when no scaffold was requested or the
    /// scaffold failed and the row was kept anyway.
    pub bootstrap_pr_url: Option<String>,
    /// Set by `POST /api/admin/apps/{id}/publish`. NULL means the app
    /// is in draft — only app admins can serve it. Once set, the
    /// app is reachable to any org member of the owning org (subject
    /// to the same membership / oxy-access checks). Re-publishing
    /// bumps the timestamp; unpublishing nulls it.
    pub published_at: Option<DateTimeWithTimeZone>,
    /// Stable identifier for the bundle's location in the customer-apps
    /// git repo, in `<repo-org>/<repo-slug>` form (e.g. `acme/dashboard`
    /// for `apps/acme/dashboard/` in the repo). Drives the S3 key
    /// (`customer-apps/<repo_path>/{draft,published}/...`) so the bundle
    /// has the same storage path across every environment regardless of
    /// what each env named the admin row. NULL means use the legacy
    /// `apps/<org_slug>/<app_slug>/` layout — only meaningful on
    /// S3-sourced rows since v0/local sources don't read from S3.
    pub repo_path: Option<String>,
    /// Points at the `app_builds` row currently serving the `draft`
    /// channel (admin preview via `?channel=draft`). Set on every
    /// `oxy publish`. NULL until the first publish in the new pipeline;
    /// legacy `s3` rows keep NULL and fall back to state-dir serving.
    pub draft_build_id: Option<Uuid>,
    /// Points at the `app_builds` row currently serving the `published`
    /// channel (what viewers see). Promote = set this to the draft
    /// pointer; unpublish = NULL. Visibility still keys off
    /// `published_at`; this names *which* bytes are live.
    pub published_build_id: Option<Uuid>,
    /// User who last made a build live (promote draft or Make Live/rollback),
    /// and when. Unlike `app_builds.published_by` (the build's original
    /// publisher), this captures the *promotion* event. FK → `users`
    /// `ON DELETE SET NULL`. NULL until the first promote.
    pub last_promoted_by: Option<Uuid>,
    pub last_promoted_at: Option<DateTimeWithTimeZone>,
    /// Who may open this app: `org` (default — any member of the owning org,
    /// the historical behavior) or `members` (only rows in `app_members`, plus
    /// the org owner and Oxy staff as break-glass). Read as a fact by
    /// `server::authz::loader`; the rule itself lives in `oxy-authz`
    /// (`Ring::AppAccess`). Kept as text with a DB CHECK rather than a PG enum
    /// so adding a tier later doesn't need a type migration.
    #[sea_orm(default_value = "org")]
    pub visibility: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl Model {
    /// True when this app is restricted to explicitly-listed members
    /// (`visibility = 'members'`). Anything unrecognized reads as unrestricted,
    /// matching the column default — a garbled value must not silently lock an
    /// org out of its own app.
    pub fn is_restricted(&self) -> bool {
        self.visibility == "members"
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrgId",
        to = "super::organizations::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Organizations,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organizations.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
