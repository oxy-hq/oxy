use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "edge_boxes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub site_id: Uuid,
    pub hardware_model: String,
    pub image_tag: String,
    pub cohort: String,
    /// IP stored as TEXT for portability (PostgreSQL `INET` columns require
    /// either the `with-ipnetwork` SeaORM feature or app-side mapping).
    pub tailscale_ip: Option<String>,
    /// `<box-hostname>.<tailnet>.ts.net` — populated from the edge
    /// worker's `X-Edge-Funnel-Hostname` header on the first
    /// authenticated `/control/*` call (mirrors `tailscale_ip`).
    /// Required for the WebRTC session endpoint to hand the browser
    /// a public-internet URL.
    pub funnel_hostname: Option<String>,
    /// `'pending' | 'active' | 'offline' | 'retired'`.
    pub status: String,
    /// UniFi Site Manager `console_id` (e.g. `F492...:7632...`). Set during
    /// onboarding when this edge_box represents a UniFi controller.
    pub unifi_console_id: Option<String>,
    pub unifi_public_ip: Option<String>,
    pub unifi_rtsp_reachable: bool,
    pub registered_at: DateTimeWithTimeZone,
    pub last_seen_at: Option<DateTimeWithTimeZone>,
    /// Total bytes MTX has sent in the last ~5-minute window —
    /// stamped by `POST /control/boxes/{id}/bandwidth` from the
    /// worker's bandwidth reporter. Overcount of Funnel-attributable
    /// bytes (also counts internal tailnet HLS proxying), but the
    /// safe-side error for "approaching quota?" alarms.
    pub bandwidth_5min_bytes: Option<i64>,
    pub bandwidth_reported_at: Option<DateTimeWithTimeZone>,
    /// IoT Phase 2 — software-update channel. Operator sets this
    /// (Settings → Cameras → Edge box → Set target); the on-device
    /// update agent polls `/control/boxes/{id}/target` and pulls
    /// when it differs from `current_image_tag`. NULL = no
    /// automated update (legacy ssh-based deploy).
    pub target_image_tag: Option<String>,
    /// Last image tag the device reported running via
    /// `/control/boxes/{id}/health`. Compared to `target_image_tag`
    /// in the UI to show pending-update state.
    pub current_image_tag: Option<String>,
    /// Pause-window. If `held_until > now()`, the target endpoint
    /// surfaces it so the device agent can skip an attempt — useful
    /// for "don't touch this box during lunch rush." NULL = no hold.
    pub held_until: Option<DateTimeWithTimeZone>,
    /// `applied` | `reverted` | `in_progress`. Set by the device
    /// via the health endpoint after an update attempt.
    pub last_update_result: Option<String>,
    pub last_update_at: Option<DateTimeWithTimeZone>,
    /// CI #3 — compatibility manifest the worker copied out of
    /// `/opt/oxy-edge/compatibility.json` in its container image.
    /// Shape: `{edge_version, git_sha, built_at, protocol_version,
    /// min_server_version}`. Nullable because older workers (built
    /// before this plumbing existed) don't include it.
    pub edge_compatibility_json: Option<serde_json::Value>,
    /// P1 #3 — non-empty when the worker's posted `protocol_version`
    /// is below the server's minimum (`OXY_MIN_EDGE_PROTOCOL_VERSION`).
    /// Human-readable reason that surfaces as an amber badge in the
    /// operator UI. NULL = compatible or no manifest yet.
    pub incompatible_reason: Option<String>,
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
    #[sea_orm(has_many = "super::cameras::Entity")]
    Cameras,
}

impl Related<super::sites::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Site.def()
    }
}

impl Related<super::cameras::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Cameras.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
