//! `SeaORM` Entity for `cli_device_requests` — a pending approval of the
//! device-code flow (`oxyc … --no-browser`). The CLI holds the device code
//! (only its SHA-256 is stored) and the PKCE verifier; a person approves the
//! request by its user code from any browser. See the migration
//! `m20261021_000001_cli_device_requests` for each column's contract.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "cli_device_requests")]
pub struct Model {
    /// SHA-256 of the device code the CLI polls with.
    #[sea_orm(primary_key, auto_increment = false)]
    pub device_code_hash: Vec<u8>,
    /// The code a person compares on the page: eight characters, no dash.
    pub user_code: String,
    /// `base64url(sha256(code_verifier))`, no padding.
    pub code_challenge: String,
    /// The host the CLI runs on, as it named itself.
    pub hostname: String,
    /// The `/cli-auth` parameters the CLI asked with, as sent.
    #[sea_orm(column_type = "JsonBinary")]
    pub ask: Json,
    /// `pending`, `approved` or `denied`.
    pub status: String,
    /// Who decided: the approver, whose token the CLI collects.
    pub user_id: Option<Uuid>,
    /// What was approved, checked under the approver's session. `None` for a
    /// login, and for a request not approved.
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub mint: Option<Json>,
    /// Why the request was refused, for the polling CLI to say.
    pub denial: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub expires_at: DateTimeWithTimeZone,
    pub decided_at: Option<DateTimeWithTimeZone>,
    pub last_polled_at: Option<DateTimeWithTimeZone>,
    /// Set once the CLI collects the approval, or burns the row with a wrong
    /// verifier. Nothing is collected twice.
    pub consumed_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
