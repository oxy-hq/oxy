use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "cameras")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub site_id: Uuid,
    /// SET NULL on edge_box delete — cameras survive box replacement and
    /// get re-bound to the new box on next register.
    pub edge_box_id: Option<Uuid>,
    pub name: String,
    /// May be empty when the camera was imported via inventory but the
    /// rtspAlias hasn't been fetched yet (e.g. owner key not yet available).
    pub rtsp_url: Option<String>,
    pub substream_url: Option<String>,
    /// Reference into the secret store; never the secret itself.
    pub credentials_ref: Option<String>,
    /// Array of {zone_id, name, polygon: [[x,y], ...]}.
    pub zones_json: Json,
    /// Array of {line_id, name, p1, p2, direction}.
    pub lines_json: Json,
    /// Defaults FALSE on inventory-imported cameras; operator flips per-camera.
    pub analytics_consent: bool,
    pub active: bool,
    /// Vendor-specific: UniFi Protect mongo ObjectId. Used by the connector
    /// proxy to fetch the rtspAlias / RTSPS URL.
    pub protect_camera_id: Option<String>,
    /// Stored as TEXT (e.g. `7483C28FA7FE`); validated app-side.
    pub mac_address: Option<String>,
    /// e.g. `UVC G5 Dome Ultra`. Snapshot from inventory.
    pub model: Option<String>,
    /// Online/offline snapshot from last inventory sync.
    pub online: Option<bool>,
    /// Free-text role for prompt selection: `kitchen`, `prep`, `dining`,
    /// `other`, or NULL. The edge worker keys into a prompt registry
    /// by this; unknown / NULL falls back to the whole-frame default.
    pub role: Option<String>,
    /// Operator zone-approval workflow: an upstream (agent / heuristic)
    /// drops candidate zone/line geometry here; the operator reviews
    /// and approves or rejects via the UI. NULL = no proposal
    /// pending — distinct from "proposed an empty array" which would
    /// CLEAR the live geometry on approval.
    pub proposed_zones_json: Option<Json>,
    pub proposed_lines_json: Option<Json>,
    /// When the proposal was set. Lets the UI age out stale
    /// proposals ("proposed 3h ago" reads less trustworthy than
    /// "proposed just now").
    pub proposed_at: Option<DateTimeWithTimeZone>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::sites::Entity",
        from = "Column::SiteId",
        to = "super::sites::Column::Id",
        on_delete = "Cascade"
    )]
    Site,
    #[sea_orm(
        belongs_to = "super::edge_boxes::Entity",
        from = "Column::EdgeBoxId",
        to = "super::edge_boxes::Column::Id",
        on_delete = "SetNull"
    )]
    EdgeBox,
}

impl Related<super::sites::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Site.def()
    }
}

impl Related<super::edge_boxes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::EdgeBox.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
