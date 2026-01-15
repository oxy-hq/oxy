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
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct DatabaseStatus {
    pub connected: bool,
    pub message: Option<String>,
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
