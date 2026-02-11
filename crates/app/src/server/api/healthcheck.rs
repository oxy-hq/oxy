use axum::{http::StatusCode, response::Json};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: u64,
    pub service: String,
    pub version: String,
    pub database: DatabaseStatus,
    pub build_info: BuildInfo,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct BuildInfo {
    pub git_commit: String,
    pub git_commit_short: String,
    pub build_timestamp: String,
    pub build_profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_url: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct DatabaseStatus {
    pub connected: bool,
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct VersionResponse {
    pub version: String,
    pub service: String,
    pub build_info: BuildInfo,
}

/// Health check endpoint
///
/// Returns the health status of the Oxy service including database connectivity.
/// This endpoint does not require authentication and can be used by:
/// - Load balancers for health checks
/// - Monitoring systems for uptime tracking
/// - Kubernetes liveness/readiness probes
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthCheckResponse),
        (status = 503, description = "Service is unhealthy", body = HealthCheckResponse)
    )
)]
pub async fn health_check()
-> Result<Json<HealthCheckResponse>, (StatusCode, Json<HealthCheckResponse>)> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let version = env!("CARGO_PKG_VERSION").to_string();

    // Build information from compile-time environment variables
    let git_commit = env!("GIT_HASH_LONG").to_string();
    let git_commit_short = env!("GIT_HASH").to_string();
    let github_server = env!("GITHUB_SERVER_URL");
    let github_repo = env!("GITHUB_REPOSITORY");
    let github_run_id = env!("GITHUB_RUN_ID");

    // Build GitHub URLs if we have the necessary info
    // Note: build.rs sets "dev" for local builds, CI sets "unknown" when unavailable
    let commit_url = if !github_server.is_empty()
        && !github_repo.is_empty()
        && git_commit != "unknown"
        && git_commit != "dev"
    {
        Some(format!(
            "{}/{}/commit/{}",
            github_server, github_repo, git_commit
        ))
    } else {
        None
    };

    let workflow_url = if !github_server.is_empty()
        && !github_repo.is_empty()
        && !github_run_id.is_empty()
        && github_run_id != "unknown"
    {
        Some(format!(
            "{}/{}/actions/runs/{}",
            github_server, github_repo, github_run_id
        ))
    } else {
        None
    };

    let build_info = BuildInfo {
        git_commit,
        git_commit_short,
        build_timestamp: env!("BUILD_TIMESTAMP").to_string(),
        build_profile: env!("BUILD_PROFILE").to_string(),
        commit_url,
        workflow_url,
    };

    // Check database connectivity
    let db_status = check_database_connection().await;

    let status = if db_status.connected {
        "healthy"
    } else {
        "unhealthy"
    };

    let response = HealthCheckResponse {
        status: status.to_string(),
        timestamp,
        service: "oxy".to_string(),
        version,
        database: db_status,
        build_info,
    };

    if status == "healthy" {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}

async fn check_database_connection() -> DatabaseStatus {
    match oxy::database::client::establish_connection().await {
        Ok(db) => {
            // Try a simple query to verify the connection is actually working
            match sea_orm::DatabaseConnection::ping(&db).await {
                Ok(_) => DatabaseStatus {
                    connected: true,
                    message: Some("Database connection successful".to_string()),
                },
                Err(e) => {
                    tracing::error!("Database ping failed: {}", e);
                    DatabaseStatus {
                        connected: false,
                        message: Some(format!("Database ping failed: {}", e)),
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to establish database connection: {}", e);
            DatabaseStatus {
                connected: false,
                message: Some(format!("Database connection failed: {}", e)),
            }
        }
    }
}

/// Readiness check endpoint
///
/// Similar to health check but specifically designed for Kubernetes readiness probes.
/// Returns 200 only when the service is ready to accept traffic.
#[utoipa::path(
    get,
    path = "/ready",
    tag = "Health",
    responses(
        (status = 200, description = "Service is ready"),
        (status = 503, description = "Service is not ready")
    )
)]
pub async fn readiness_check() -> StatusCode {
    match check_database_connection().await {
        db_status if db_status.connected => StatusCode::OK,
        _ => StatusCode::SERVICE_UNAVAILABLE,
    }
}

/// Liveness check endpoint
///
/// Minimal check to verify the service process is alive.
/// This endpoint always returns 200 if the service is running.
/// Designed for Kubernetes liveness probes.
#[utoipa::path(
    get,
    path = "/live",
    tag = "Health",
    responses(
        (status = 200, description = "Service is alive")
    )
)]
pub async fn liveness_check() -> StatusCode {
    StatusCode::OK
}

/// Version information endpoint
///
/// Returns version and build information without any health checks.
/// This endpoint always returns 200 as long as the service is running,
/// making it reliable for displaying diagnostics even when the service is unhealthy.
#[utoipa::path(
    get,
    path = "/version",
    tag = "Health",
    responses(
        (status = 200, description = "Version information", body = VersionResponse)
    )
)]
pub async fn version_info() -> Json<VersionResponse> {
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Build information from compile-time environment variables
    let git_commit = env!("GIT_HASH_LONG").to_string();
    let git_commit_short = env!("GIT_HASH").to_string();
    let github_server = env!("GITHUB_SERVER_URL");
    let github_repo = env!("GITHUB_REPOSITORY");
    let github_run_id = env!("GITHUB_RUN_ID");

    // Build GitHub URLs if we have the necessary info
    // Note: build.rs sets "dev" for local builds, CI sets "unknown" when unavailable
    let commit_url = if !github_server.is_empty()
        && !github_repo.is_empty()
        && git_commit != "unknown"
        && git_commit != "dev"
    {
        Some(format!(
            "{}/{}/commit/{}",
            github_server, github_repo, git_commit
        ))
    } else {
        None
    };

    let workflow_url = if !github_server.is_empty()
        && !github_repo.is_empty()
        && !github_run_id.is_empty()
        && github_run_id != "unknown"
    {
        Some(format!(
            "{}/{}/actions/runs/{}",
            github_server, github_repo, github_run_id
        ))
    } else {
        None
    };

    let build_info = BuildInfo {
        git_commit,
        git_commit_short,
        build_timestamp: env!("BUILD_TIMESTAMP").to_string(),
        build_profile: env!("BUILD_PROFILE").to_string(),
        commit_url,
        workflow_url,
    };

    Json(VersionResponse {
        version,
        service: "oxy".to_string(),
        build_info,
    })
}
