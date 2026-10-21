//! The staff console's publish and unpublish, as the port the partner console
//! calls (`oxy_app_core::app_lifecycle::AppLifecycle`).

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use entity::apps;
use oxy_app_core::app_lifecycle::{AppLifecycle, LifecycleFuture};
use oxy_app_core::audit::RequestActor;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::admin::apps::handlers::{AuditedBy, PromoteRefusal, publish_one, unpublish_one};

/// [`publish_one`] / [`unpublish_one`] with the caller recording its own audit row.
pub struct Lifecycle;

impl AppLifecycle for Lifecycle {
    fn publish<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        app_id: Uuid,
        actor: &'a RequestActor,
    ) -> LifecycleFuture<'a, Result<apps::Model, Response>> {
        Box::pin(async move {
            publish_one(db, app_id, actor, AuditedBy::Caller)
                .await
                .map_err(|e| PromoteRefusal::from(e).into_response())
        })
    }

    fn unpublish<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        app_id: Uuid,
        actor: &'a RequestActor,
    ) -> LifecycleFuture<'a, Result<apps::Model, StatusCode>> {
        Box::pin(async move {
            unpublish_one(db, app_id, actor, AuditedBy::Caller)
                .await
                .map_err(|e| e.status)
        })
    }
}
