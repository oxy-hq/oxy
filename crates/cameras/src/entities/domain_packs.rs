use sea_orm::entity::prelude::*;

/// Workspace-scoped domain pack: a bundle defining what the camera
/// fleet platform is *for* in this workspace (camera roles, VLM
/// prompts, output schema, pack-wide variables).
///
/// One workspace can have multiple packs (e.g. forked starters),
/// but only one is `is_active = true` at any moment. A partial
/// unique index enforces that invariant at the DB level —
/// `set_active` must always run inside a transaction that
/// deactivates the previous active row first.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "camera_domain_packs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Loose FK to `workspaces.id` (cross-aggregate; no constraint
    /// per `domain-boundaries.md` P3). Cleanup is application-level.
    pub workspace_id: Uuid,
    /// Logical identifier within the workspace. Unique together with
    /// `workspace_id`. Operators see this in the picker; doubles as
    /// the lookup key for `cameras.role` in v2.
    pub pack_key: String,
    pub name: String,
    pub description: Option<String>,
    /// Tracked starter key (e.g. `"restaurant"`). Combined with
    /// `starter_version` this tells the curator UI which upstream
    /// to diff against when Oxy ships an update.
    pub starter_key: String,
    /// Semver of the starter at fork time. Bumps when the operator
    /// accepts an Oxy update via the diff modal.
    pub starter_version: String,
    /// True after the first operator edit. UI uses this to show
    /// "discard local changes" alongside "accept Oxy update."
    pub locally_modified: bool,
    /// Full pack body as JSON. See `service::packs::body::PackBody`
    /// for the typed schema (deserialize on read, serialize on
    /// write). Stored as JSONB to keep the door open for future
    /// path-indexed queries (`body -> 'roles' -> ?`).
    pub body: Json,
    /// At most one row per workspace has this true (enforced by a
    /// partial unique index). The worker only ever reads the
    /// active pack.
    pub is_active: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
