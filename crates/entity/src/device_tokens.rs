//! Where a push would go, if push delivery is configured.
//!
//! Collected independently of whether anything can send yet: a token is only
//! obtainable from a device the user is holding, so gathering them has to work
//! before delivery does, or the first thing a push integration needs is a
//! user base that has already granted permission — which it cannot have.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "device_tokens")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    /// `apns` | `fcm` | `web`.
    ///
    /// Web push is a real third platform rather than a variant: different key
    /// format, different endpoint, and it is what an installed PWA uses — which
    /// is the surface the frontline pilot actually ships on.
    pub platform: String,
    pub token: String,
    pub device_name: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub last_seen_at: DateTimeWithTimeZone,
    /// `false` stops push to this device without unregistering it.
    pub enabled: bool,
    /// Push here only at this level and above (`passive` | `active` |
    /// `time_sensitive` | `critical`); `None` for everything.
    pub min_level: Option<String>,
    /// What the device last reported of its OS permission: `granted` |
    /// `denied` | `provisional` | `ephemeral` | `unknown`. `None` before it
    /// has reported.
    pub os_permission: Option<String>,
    /// The `<app id|oxygen>.<kind>` channel or category ids (or a whole
    /// `<app id|oxygen>` group) the OS has switched off, as a JSON array of
    /// strings. `None` for none.
    pub os_disabled: Option<Json>,
    pub app_version: Option<String>,
    /// A `web` row's subscription keys (`PushSubscription.toJSON().keys`),
    /// base64url: the browser's P-256 public key and its 16-byte auth secret.
    /// With both, a Web Push carries an encrypted payload (RFC 8291); without
    /// them — a native row, or a browser registered before keys were stored —
    /// it is a bodiless wake.
    pub web_push_p256dh: Option<String>,
    pub web_push_auth: Option<String>,
}

impl ActiveModelBehavior for ActiveModel {}
