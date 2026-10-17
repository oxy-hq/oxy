use sea_orm::entity::prelude::*;

/// A person's own notification settings, above every source: pause, quiet
/// hours and the timezone those are read in. One row per person, written
/// lazily; no row is "nothing set".
///
/// `version` changes on every write to the person's settings or devices, and
/// is the `ETag` of `GET /api/notifications/settings`.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "notification_user_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: Uuid,
    pub version: i64,
    /// Push, email and Slack are held back for everything but critical
    /// until then.
    pub paused_until: Option<DateTimeWithTimeZone>,
    /// IANA, e.g. `Europe/Stockholm`. Required while quiet hours are set.
    pub timezone: Option<String>,
    /// Minutes after local midnight. The four quiet-hours columns are set
    /// together or not at all.
    pub quiet_start: Option<i16>,
    pub quiet_end: Option<i16>,
    /// The days a window starts on, as bits: bit 0 is Sunday.
    pub quiet_days: Option<i16>,
    /// `critical` | `time_sensitive`: the lowest level that still comes
    /// through.
    pub quiet_allow: Option<String>,
    /// Pushes held by quiet hours up to here have been summarized.
    pub quiet_summary_through: Option<DateTimeWithTimeZone>,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
