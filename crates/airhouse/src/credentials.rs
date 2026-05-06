//! Resolver for `airhouse_managed` database connections.
//!
//! Looks up the requesting oxy user's provisioned Airhouse user row, decrypts
//! the password from `org_secrets`, and returns connection coordinates ready
//! for the connector layer.

use oxy_platform::db::establish_connection;
use oxy_platform::secrets::OrgSecretsService;
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::entity::users::{self as airhouse_users, AirhouseUserStatus};
use crate::entity::{Tenants, Users};

// Duplicated from `crate::config` (which is `admin`-feature-gated and not
// available under `credentials`-only builds). The `config` module's REQUIRED_VARS
// list includes these same names; both copies must stay in sync.
const AIRHOUSE_WIRE_HOST_VAR: &str = "AIRHOUSE_WIRE_HOST";
const AIRHOUSE_WIRE_PORT_VAR: &str = "AIRHOUSE_WIRE_PORT";
const DEFAULT_WIRE_PORT: u16 = 5445;

/// Resolved coordinates for an `airhouse_managed` database, pulled from the
/// oxy database at connection time.
pub struct ManagedAirhouseCreds {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub username: String,
    pub password: String,
}

/// Resolve the credentials for `DatabaseType::AirhouseManaged` from oxy's own
/// database.
///
/// - When `oxy_user_id` is `Some`, filters to the active row owned by that
///   user — the cloud-mode path. The connector layer does not yet plumb
///   request-time user context, so callers currently always pass `None`.
/// - When `oxy_user_id` is `None`, falls back to single-row resolution: the
///   query must return exactly one active row. Sufficient for local mode
///   (single seeded nil-org user) but errors in any multi-user deployment.
pub async fn resolve_managed_airhouse_credentials(
    oxy_user_id: Option<uuid::Uuid>,
) -> Result<ManagedAirhouseCreds, OxyError> {
    let host = std::env::var(AIRHOUSE_WIRE_HOST_VAR).map_err(|_| {
        OxyError::ConfigurationError(format!(
            "{AIRHOUSE_WIRE_HOST_VAR} is not set; airhouse_managed requires the Airhouse \
             integration to be configured"
        ))
    })?;
    let port: u16 = std::env::var(AIRHOUSE_WIRE_PORT_VAR)
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_WIRE_PORT);

    let conn = establish_connection().await?;
    let mut query =
        Users::find().filter(airhouse_users::Column::Status.eq(AirhouseUserStatus::Active));
    if let Some(uid) = oxy_user_id {
        query = query.filter(airhouse_users::Column::OxyUserId.eq(uid));
    }
    let rows = query
        .all(&conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query airhouse_users: {e}")))?;

    let row = match (rows.len(), oxy_user_id) {
        (0, Some(uid)) => {
            return Err(OxyError::ConfigurationError(format!(
                "airhouse_managed: no provisioned Airhouse user for oxy user {uid}. Open \
                 Settings → Airhouse and click \"Provision Airhouse access\" first."
            )));
        }
        (0, None) => {
            return Err(OxyError::ConfigurationError(
                "airhouse_managed: no provisioned Airhouse user. Open Settings → Airhouse and \
                 click \"Provision Airhouse access\" first."
                    .into(),
            ));
        }
        (1, _) => rows.into_iter().next().unwrap(),
        (n, Some(uid)) => {
            return Err(OxyError::DBError(format!(
                "airhouse_managed: invariant violated — found {n} active rows for oxy user \
                 {uid} (workspace,user) is supposed to be unique"
            )));
        }
        (n, None) => {
            return Err(OxyError::ConfigurationError(format!(
                "airhouse_managed: cannot pick a user automatically — found {n} provisioned \
                 users. The single-user resolution path is only safe in local mode; cloud / \
                 multi-user deployments must use the explicit `airhouse:` config until \
                 user-aware connector wiring lands."
            )));
        }
    };

    let secret_id = row.password_secret_id.ok_or_else(|| {
        OxyError::ConfigurationError(
            "airhouse_managed: provisioned user has no password secret. Rotate the password from \
             Settings → Airhouse to recover."
                .into(),
        )
    })?;
    let password = OrgSecretsService::get_by_id(secret_id).await?;

    let tenant = Tenants::find_by_id(row.tenant_row_id)
        .one(&conn)
        .await
        .map_err(|e| OxyError::DBError(format!("query airhouse_tenants: {e}")))?
        .ok_or_else(|| {
            OxyError::ConfigurationError(
                "airhouse_managed: tenant row missing for provisioned user".into(),
            )
        })?;

    Ok(ManagedAirhouseCreds {
        host,
        port,
        dbname: tenant.airhouse_tenant_id,
        username: row.username,
        password,
    })
}
