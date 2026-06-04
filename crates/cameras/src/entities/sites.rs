use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sites")]
pub struct Model {
    /// Aggregate root.
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    /// Loose cross-aggregate ref to `workspaces.id` (no FK constraint).
    pub workspace_id: Uuid,
    pub name: String,
    pub timezone: String,
    pub region: Option<String>,
    /// Provenance of this site row. `manual` (operator-created)
    /// or `unifi` (imported via the UniFi onboarding flow). Drives
    /// UI badges and gates re-import to UniFi-only rows.
    pub source: String,
    /// WAN-facing IP for the site's network — set by the operator when
    /// the customer hasn't received an edge box yet but wants to begin
    /// processing right away. Powers the bulk RTSP-rewrite endpoint:
    /// each camera's LAN URL (`rtsps://192.168.x.x:7441/<id>`) becomes
    /// `rtsp://<public_ip>:7447/<id>`, assuming the customer's router
    /// forwards 7447 → UniFi controller. NULL when no NAT path is
    /// configured; the rewrite endpoint refuses to run in that case.
    pub public_ip: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::edge_boxes::Entity")]
    EdgeBoxes,
    #[sea_orm(has_many = "super::cameras::Entity")]
    Cameras,
}

impl Related<super::edge_boxes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EdgeBoxes.def()
    }
}

impl Related<super::cameras::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Cameras.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
