use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "edge_box_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub edge_box_id: Uuid,
    /// SHA-256 of the bearer. **Plaintext is never stored.** Look up by
    /// hashing the inbound bearer; rotate by deleting the row.
    #[sea_orm(unique)]
    pub token_hash: String,
    /// First 8 chars of the bearer — safe to log for support / debugging
    /// without exposing the secret.
    pub token_prefix: String,
    /// Operator-supplied label (e.g. "Almaden — primary").
    pub description: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub last_used_at: Option<DateTimeWithTimeZone>,
    /// Soft-delete marker. Once set, the token is rejected on every lookup.
    /// We keep the row around for audit purposes; cleanup happens on a
    /// scheduled sweep.
    pub revoked_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::edge_boxes::Entity",
        from = "Column::EdgeBoxId",
        to = "super::edge_boxes::Column::Id",
        on_delete = "Cascade"
    )]
    EdgeBox,
}

impl Related<super::edge_boxes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EdgeBox.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
