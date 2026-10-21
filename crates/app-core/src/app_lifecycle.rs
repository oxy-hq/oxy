//! Publishing and unpublishing an app on a caller's behalf, as the object a
//! surface that is not the custom-app console calls it through.
//!
//! The partner console (`oxy-api-tenancy`) publishes and unpublishes a managed
//! client's app through the same functions the staff console does. Those are in
//! `oxy-api-custom-apps`, and one surface crate may not depend on another (rule S3
//! in `internal-docs/domain-boundaries.md`), so the console's side of the call is
//! this port: defined here, below both, implemented by `oxy-api-custom-apps`
//! (`app_lifecycle::Lifecycle`), and layered onto the partner routes as an
//! `Extension` by the composition root (`oxy-server`).

use std::future::Future;
use std::pin::Pin;

use axum::http::StatusCode;
use axum::response::Response;
use entity::apps;
use sea_orm::DatabaseConnection;
use uuid::Uuid;

use crate::audit::RequestActor;

/// A boxed future, as the port's methods return it.
pub type LifecycleFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Make an app live to its viewers, or take it out of view, recording the audit
/// row as the CALLER's own (`AuditedBy::Caller`: the caller writes its own
/// `partner.app.*` row beside the move).
pub trait AppLifecycle: Send + Sync {
    /// Repoint production at the draft build under the app's row lock, stamping
    /// `last_promoted_*` and dropping the per-app caches. A refusal is returned
    /// whole as the response the staff console answers with, body included: a
    /// draft a sandbox agent token published is `409 draft_published_by_agent`.
    fn publish<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        app_id: Uuid,
        actor: &'a RequestActor,
    ) -> LifecycleFuture<'a, Result<apps::Model, Response>>;

    /// Take production's build away (no dangling pointer) under the same lock.
    /// Every failure is the bare status it always was.
    fn unpublish<'a>(
        &'a self,
        db: &'a DatabaseConnection,
        app_id: Uuid,
        actor: &'a RequestActor,
    ) -> LifecycleFuture<'a, Result<apps::Model, StatusCode>>;
}
