use sea_orm::entity::prelude::*;

/// Per-workspace pointer to a stored UniFi Site Manager API key.
///
/// The key plaintext lives in `org_secrets` (encrypted at rest with the
/// deployment master key); this row only carries the workspace→secret
/// lookup + minimal audit metadata so the operator UI can answer
/// "is a key configured? when was it last touched?" without ever
/// reading the secret itself.
///
/// `org_id` is denormalized so the lookup from `workspace_id` doesn't
/// need a join through the workspaces aggregate every time. It's
/// stamped on create and treated as immutable (a workspace doesn't
/// move between orgs in any flow we ship today).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "unifi_credentials")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub org_id: Uuid,
    /// Points into `org_secrets.id`. Resolved via
    /// `OrgSecretsService::get_by_id` when an authenticated camera
    /// service path needs the plaintext.
    pub secret_id: Uuid,
    /// Operator who PUT this credential. NULL when the path didn't
    /// resolve a user (e.g. local-mode deployments where every
    /// request is unauthenticated).
    pub set_by_user_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
