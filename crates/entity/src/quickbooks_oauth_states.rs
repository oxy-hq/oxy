use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Short-lived CSRF/state row for the QuickBooks OAuth authorization-code
/// flow. Created (authenticated) when the user clicks Connect, consumed
/// once by the public callback. Carries no secret material — only the var
/// names the callback resolves/writes through the workspace secret manager.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "quickbooks_oauth_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub nonce: String,
    pub project_id: Uuid,
    pub client_id: String,
    pub client_secret_var: String,
    pub refresh_token_var: String,
    /// Exact redirect URI sent to Intuit (computed from the authorize
    /// request host). Reused verbatim in the token exchange — Intuit
    /// requires an exact match.
    pub redirect_uri: String,
    /// `popup` | `redirect` — drives how the success page returns.
    pub mode: String,
    /// Where to send the browser back in `redirect` (mobile) mode.
    pub return_path: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
    pub expires_at: DateTimeWithTimeZone,
    pub consumed_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
