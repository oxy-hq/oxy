//! `/api/admin/assume` — explicit, audited, time-bounded **assume-role** for Oxy
//! staff.
//!
//! ## Why this exists
//!
//! Oxy staff have ALWAYS been able to act as a tenant Owner: `org_context` and
//! `workspace_context` synthesize an Owner `org_members` row for any Global Owner
//! / Global Admin who isn't a real member (`is_global_override = true`). That is
//! impersonation — it was just **implicit, unbounded, unlogged and unannounced**.
//! You opened a tenant and silently became its Owner.
//!
//! This module doesn't add a new power. It makes the existing one *observable*:
//!
//! * **Opt-in** — no live session for `(actor, org)` ⇒ **no synthetic membership**
//!   ⇒ the operator is a plain non-member and gets a 403, exactly like anyone
//!   else. The seam is now closed by default.
//! * **Scoped** — one org per session. No blanket cross-tenant reach.
//! * **Bounded** — [`MAX_SESSION`] minutes, non-renewable (start a fresh one).
//! * **Explained** — a `reason` is required.
//! * **Audited** — start and end both write `audit_events`, with the actor being
//!   the REAL staff user. The audit trail never names the impersonated identity as
//!   the actor, or the log would launder the impersonation.
//! * **Announced** — the frontend renders a persistent banner while it's live.
//!
//! ## What it deliberately does NOT do: act as a *user*
//!
//! Assume-role names an **org**, never a person. "Act as bob@acme" was designed,
//! scoped read-only, and then **rejected** (2026-07-14) — the schema for it was
//! added and dropped again (`m20260714_000007`). The reasoning, so it isn't
//! relitigated from scratch:
//!
//! Acting as an org is bounded on both ends: the session names one org, and the
//! role it grants is capped — Owner for staff, Admin for a partner — with
//! `is_global_override` set, so `OrgAdminStrict` still shuts billing and
//! admin-promotion. A **user** swap has neither bound. You become a specific
//! person, and every ownership check in the product silently resolves to them.
//!
//! Read-only would have contained the worst of it (a write recorded as *Bob did
//! this* is forgery, not impersonation) — but read-only is a property you must keep
//! true across every endpoint that will ever exist. The cost of getting it wrong
//! once is a customer's data touched under their own name, in a log that says they
//! did it. Debuggability is worth a great deal; it is not worth an authorization
//! primitive whose safety depends on nobody ever adding a handler that forgets.
//!
//! The debugging need is real, so meet it another way: reproduce with the *role*,
//! not the *identity* (act as the org, then read the member's effective
//! permissions), or ask the customer to share what they see. If that proves
//! insufficient, revisit this with a design that does not depend on a global
//! read-only invariant.
//!
//! ## What it deliberately does NOT change
//!
//! * **Strict guards stay strict.** `OrgAdminStrict` / `OrgMemberStrict` still
//!   reject `is_global_override`, so billing and admin-promotion remain closed
//!   under an assumed identity. Assuming a role does not launder you into a
//!   tenant officer.
//! * **The customer's Oxy-staff lockdown still holds.** Custom-app access is
//!   gated on `app_admins` + `workspace_oxy_lockdown` and on *real*
//!   `org_members` rows — never on the synthetic one — so assuming a role is not
//!   a way around a customer's lockdown.

use axum::Json;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Router, extract::Query};
use chrono::{Duration, Utc};
use entity::admin_assume_sessions;
use entity::prelude::{AdminAssumeSessions, Organizations};
use oxy::database::client::establish_connection;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, Condition, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::server::api::audit::{self, ActorType, AuditEntry};
use crate::server::api::middlewares::oxy_owner_guard::is_oxy_owner;
use crate::server::router::AppState;

/// Hard ceiling on a session. Non-renewable — an operator who needs longer starts
/// a new (separately audited) session, so a long investigation leaves a trail of
/// deliberate re-entries rather than one silently-extended grant.
const MAX_SESSION: i64 = 60;

/// Mounted at `/api/assume`, NOT under `/admin`.
///
/// It cannot live under the admin surface: a partner acting as a client is not
/// staff, so `/admin/*` would 403 them — they could start a session and never end
/// it. And staff can't reach `/admin/*` while acting either, because acting closes
/// the staff surface. Either way, putting the exit behind the door it locks is a
/// trap.
///
/// Authorization lives in the handlers (`may_act_as`), which is where it belongs
/// now that two different populations can act.
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/assume", post(start).delete(end))
        .route("/assume/current", get(current))
        // The full impersonation log is a staff view — a partner has no business
        // reading who else Oxy has acted as.
        .route("/assume/history", get(history))
}

// Keep clippy quiet about the unused imports when the router shrinks.
#[allow(unused_imports)]
use axum::routing as _routing;

#[derive(Deserialize)]
pub struct StartBody {
    pub org_id: Uuid,
    /// Required. An unexplained impersonation is a red flag, not a convenience.
    pub reason: String,
}

#[derive(Serialize)]
pub struct SessionDto {
    pub id: Uuid,
    pub org_id: Uuid,
    pub org_name: Option<String>,
    /// Where the operator lands: the org's own product surface is `/{slug}`.
    pub org_slug: Option<String>,
    /// True when the assumed org holds a partner grant — then the surface that
    /// matters is the partner console, not the org home. Acting as a partner and
    /// landing on an org dashboard would show you the wrong product.
    pub is_partner: bool,
    pub actor_email: String,
    pub reason: String,
    pub started_at: String,
    pub expires_at: String,
    /// Convenience for the banner countdown.
    pub expires_in_seconds: i64,
}

fn db_err<E: std::fmt::Display>(ctx: &str) -> impl Fn(E) -> StatusCode + '_ {
    move |e| {
        tracing::error!("admin/assume: {ctx}: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    }
}

async fn db() -> Result<DatabaseConnection, StatusCode> {
    establish_connection().await.map_err(|e| {
        tracing::error!("admin/assume: DB connect failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// **Who may act as an org.** Two populations, one rule each:
///
/// * **Oxy staff** — any org. They always could (the synthetic-Owner seam);
///   assume-role just makes it deliberate and audited.
/// * **A partner** — only the clients they are assigned to, and only with
///   `develop_apps`. Entering a client's product IS the data plane: it means
///   reading their warehouse, their threads, their dashboards. An
///   `account_manager` — whose whole point is "runs the relationship, cannot touch
///   the data" — must not be able to walk in through this door, or the role split
///   is decorative.
///
/// Re-checked on **every request** (not just at session creation), so revoking a
/// partner's `develop_apps` or un-assigning a client kills a live session's reach
/// immediately rather than at expiry.
pub async fn may_act_as(
    db: &DatabaseConnection,
    user_id: Uuid,
    user_email: &str,
    org_id: Uuid,
) -> Option<ActingAs> {
    if is_oxy_owner(user_email)
        || crate::server::api::customer_apps_auth::is_app_admin_email(db, user_email)
            .await
            .unwrap_or(false)
    {
        return Some(ActingAs::Staff);
    }

    // A partner acting as one of its clients.
    use crate::server::api::middlewares::partner_authz::{
        PartnerCapability, partner_for_org, resolve_scope,
    };
    let partner_org_id = partner_for_org(db, org_id).await?;
    let scope = resolve_scope(db, partner_org_id, user_id, user_email).await?;
    if !scope.allows(PartnerCapability::DevelopApps) {
        return None;
    }
    if !scope.org_ids.contains(&org_id) {
        return None;
    }
    Some(ActingAs::Partner)
}

/// Which authority a live session rests on — it decides the role the request runs
/// with, so it must not be guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActingAs {
    Staff,
    Partner,
}

impl ActingAs {
    /// Staff get Owner (what the old silent override gave them). A partner gets
    /// **Admin**: enough to work inside the client's product, but the org's own
    /// Owner remains untouchable — a partner administers a client, it never owns
    /// one. Both are marked `is_global_override`, so `OrgAdminStrict` still shuts
    /// billing and admin-promotion to them.
    pub fn org_role(self) -> entity::org_members::OrgRole {
        match self {
            ActingAs::Staff => entity::org_members::OrgRole::Owner,
            ActingAs::Partner => entity::org_members::OrgRole::Admin,
        }
    }
}

/// A session is live when it hasn't been ended and hasn't expired.
fn live_filter(now: chrono::DateTime<chrono::FixedOffset>) -> Condition {
    Condition::all()
        .add(admin_assume_sessions::Column::EndedAt.is_null())
        .add(admin_assume_sessions::Column::ExpiresAt.gt(now))
}

/// **The enforcement primitive.** `org_context` / `workspace_context` call this
/// before synthesizing an Owner membership: no live session for `(actor, org)`,
/// no override. Fails CLOSED — a DB error denies the override rather than
/// silently granting cross-tenant reach.
pub async fn is_session_live(db: &DatabaseConnection, actor_user_id: Uuid, org_id: Uuid) -> bool {
    let now = Utc::now().fixed_offset();
    match AdminAssumeSessions::find()
        .filter(admin_assume_sessions::Column::ActorUserId.eq(actor_user_id))
        .filter(admin_assume_sessions::Column::OrgId.eq(org_id))
        .filter(live_filter(now))
        .one(db)
        .await
    {
        Ok(row) => row.is_some(),
        Err(e) => {
            tracing::error!("admin/assume: liveness check failed (denying): {e}");
            false
        }
    }
}

fn to_dto(
    m: admin_assume_sessions::Model,
    org: Option<&entity::organizations::Model>,
    is_partner: bool,
) -> SessionDto {
    let expires_in_seconds = (m.expires_at.to_utc() - Utc::now()).num_seconds().max(0);
    SessionDto {
        id: m.id,
        org_id: m.org_id,
        org_name: org.map(|o| o.name.clone()),
        org_slug: org.map(|o| o.slug.clone()),
        is_partner,
        actor_email: m.actor_email,
        reason: m.reason,
        started_at: m.started_at.to_rfc3339(),
        expires_at: m.expires_at.to_rfc3339(),
        expires_in_seconds,
    }
}

/// `POST /admin/assume` — begin acting as an Owner of `org_id`.
pub async fn start(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Json(body): Json<StartBody>,
) -> Result<Json<SessionDto>, StatusCode> {
    let reason = body.reason.trim().to_string();
    if reason.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let db = db().await?;

    let org = Organizations::find_by_id(body.org_id)
        .one(&db)
        .await
        .map_err(db_err("load org"))?
        .ok_or(StatusCode::NOT_FOUND)?;

    // The gate. Staff may act as any org; a partner only as an assigned client,
    // and only with `develop_apps`.
    let authority = may_act_as(&db, actor.id, &actor.email, body.org_id)
        .await
        .ok_or(StatusCode::FORBIDDEN)?;

    // Re-entering an org you're already assuming is idempotent — return the live
    // session rather than stacking rows (and rather than silently extending it).
    let now = Utc::now().fixed_offset();
    if let Some(existing) = AdminAssumeSessions::find()
        .filter(admin_assume_sessions::Column::ActorUserId.eq(actor.id))
        .filter(admin_assume_sessions::Column::OrgId.eq(body.org_id))
        .filter(live_filter(now))
        .one(&db)
        .await
        .map_err(db_err("existing session"))?
    {
        let is_partner = org_is_partner(&db, body.org_id).await;
        return Ok(Json(to_dto(existing, Some(&org), is_partner)));
    }

    let expires_at = (Utc::now() + Duration::minutes(MAX_SESSION)).fixed_offset();

    // The session and its audit row must land together. The comment below is a
    // promise — "if it can't be recorded, the session must not stand" — and a
    // separate insert + audit::record broke it: a failed audit write returned 500
    // while leaving a LIVE, unaudited impersonation behind. One transaction makes
    // the promise true: either both commit, or neither does.
    let txn = db.begin().await.map_err(db_err("begin assume"))?;

    let model = admin_assume_sessions::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        actor_user_id: ActiveValue::Set(actor.id),
        actor_email: ActiveValue::Set(actor.email.clone()),
        org_id: ActiveValue::Set(body.org_id),
        reason: ActiveValue::Set(reason.clone()),
        started_at: ActiveValue::NotSet,
        expires_at: ActiveValue::Set(expires_at),
        ended_at: ActiveValue::Set(None),
    }
    .insert(&txn)
    .await
    .map_err(db_err("insert session"))?;

    audit::record_in_txn(
        &txn,
        AuditEntry::new(actor.email.clone(), "admin.assume.started")
            .actor(actor.id, ActorType::User)
            .org(body.org_id)
            .target("organization", body.org_id.to_string(), org.name.clone())
            .reason(reason)
            .metadata(serde_json::json!({ "expires_at": expires_at.to_rfc3339() })),
    )
    .await
    .map_err(db_err("audit assume.started"))?;

    txn.commit().await.map_err(db_err("commit assume"))?;

    tracing::warn!(
        actor = %actor.email, org_id = %body.org_id, ?authority,
        "assume: STARTED — actor is now acting as this org"
    );
    let is_partner = org_is_partner(&db, body.org_id).await;
    Ok(Json(to_dto(model, Some(&org), is_partner)))
}

#[derive(Deserialize)]
pub struct EndQuery {
    /// Optional: end the session for this org. Omitted = end ALL live sessions.
    pub org_id: Option<Uuid>,
}

/// `DELETE /admin/assume` — stop acting as a tenant.
pub async fn end(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Query(q): Query<EndQuery>,
) -> Result<StatusCode, StatusCode> {
    let db = db().await?;
    let now = Utc::now().fixed_offset();

    let mut query = AdminAssumeSessions::find()
        .filter(admin_assume_sessions::Column::ActorUserId.eq(actor.id))
        .filter(live_filter(now));
    if let Some(org_id) = q.org_id {
        query = query.filter(admin_assume_sessions::Column::OrgId.eq(org_id));
    }
    let live = query.all(&db).await.map_err(db_err("load live sessions"))?;

    for s in live {
        let org_id = s.org_id;
        // Atomic, matching `start`: the end + its audit land in one transaction,
        // so a dropped `assume.ended` can't leave the impersonation log showing an
        // apparently-open session.
        let txn = db.begin().await.map_err(db_err("begin end-session"))?;
        let mut m: admin_assume_sessions::ActiveModel = s.into();
        m.ended_at = ActiveValue::Set(Some(now));
        m.update(&txn).await.map_err(db_err("end session"))?;
        audit::record_in_txn(
            &txn,
            AuditEntry::new(actor.email.clone(), "admin.assume.ended")
                .actor(actor.id, ActorType::User)
                .org(org_id)
                .target("organization", org_id.to_string(), String::new()),
        )
        .await
        .map_err(db_err("audit assume.ended"))?;
        txn.commit().await.map_err(db_err("commit end-session"))?;
        tracing::info!(actor = %actor.email, %org_id, "admin/assume: ENDED");
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /admin/assume/current` — the caller's live sessions. Drives the banner.
pub async fn current(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
) -> Result<Json<Vec<SessionDto>>, StatusCode> {
    let db = db().await?;
    let now = Utc::now().fixed_offset();

    let rows = AdminAssumeSessions::find()
        .filter(admin_assume_sessions::Column::ActorUserId.eq(actor.id))
        .filter(live_filter(now))
        .order_by_desc(admin_assume_sessions::Column::StartedAt)
        .all(&db)
        .await
        .map_err(db_err("load current"))?;

    let orgs = org_index(&db, rows.iter().map(|r| r.org_id).collect()).await?;
    let partners = partner_org_ids(&db).await;
    Ok(Json(
        rows.into_iter()
            .map(|r| {
                let is_partner = partners.contains(&r.org_id);
                let org = orgs.get(&r.org_id).cloned();
                to_dto(r, org.as_ref(), is_partner)
            })
            .collect(),
    ))
}

/// `GET /assume/history` — every session (live or not). The impersonation log an
/// operator or auditor reads. **Staff only** — it spans tenants.
pub async fn history(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
) -> Result<Json<Vec<SessionDto>>, StatusCode> {
    let db = db().await?;
    let staff = is_oxy_owner(&actor.email)
        || crate::server::api::customer_apps_auth::is_app_admin_email(&db, &actor.email)
            .await
            .unwrap_or(false);
    if !staff {
        return Err(StatusCode::FORBIDDEN);
    }
    let rows = AdminAssumeSessions::find()
        .order_by_desc(admin_assume_sessions::Column::StartedAt)
        .all(&db)
        .await
        .map_err(db_err("load history"))?;

    let orgs = org_index(&db, rows.iter().map(|r| r.org_id).collect()).await?;
    let partners = partner_org_ids(&db).await;
    Ok(Json(
        rows.into_iter()
            .map(|r| {
                let is_partner = partners.contains(&r.org_id);
                let org = orgs.get(&r.org_id).cloned();
                to_dto(r, org.as_ref(), is_partner)
            })
            .collect(),
    ))
}

async fn org_index(
    db: &DatabaseConnection,
    ids: Vec<Uuid>,
) -> Result<std::collections::HashMap<Uuid, entity::organizations::Model>, StatusCode> {
    if ids.is_empty() {
        return Ok(Default::default());
    }
    Ok(Organizations::find()
        .filter(entity::organizations::Column::Id.is_in(ids))
        .all(db)
        .await
        .map_err(db_err("load orgs"))?
        .into_iter()
        .map(|o| (o.id, o))
        .collect())
}

/// Does this org hold a partner grant? Decides which surface the operator lands on.
async fn org_is_partner(db: &DatabaseConnection, org_id: Uuid) -> bool {
    entity::prelude::PartnerGrants::find_by_id(org_id)
        .one(db)
        .await
        .ok()
        .flatten()
        .is_some_and(|g| g.status == "active")
}

async fn partner_org_ids(db: &DatabaseConnection) -> std::collections::HashSet<Uuid> {
    entity::prelude::PartnerGrants::find()
        .all(db)
        .await
        .map(|rows| {
            rows.into_iter()
                .filter(|g| g.status == "active")
                .map(|g| g.org_id)
                .collect()
        })
        .unwrap_or_default()
}

// ── acting is a MODE, not a badge ─────────────────────────────────────────

/// Every live session this actor holds. `resolve_scope` and the admin guard both
/// read it, so "am I acting?" has exactly one answer.
///
/// Fails CLOSED for authorization (empty ⇒ no synthesized reach) — but note the
/// admin guard below reads it in the opposite direction, where empty means
/// *allowed*. That asymmetry is deliberate and safe: a DB error there means staff
/// keep their normal admin access, which is the status quo, not an escalation.
pub async fn live_sessions_for(
    db: &DatabaseConnection,
    actor_user_id: Uuid,
) -> Vec<admin_assume_sessions::Model> {
    let now = Utc::now().fixed_offset();
    AdminAssumeSessions::find()
        .filter(admin_assume_sessions::Column::ActorUserId.eq(actor_user_id))
        .filter(live_filter(now))
        .all(db)
        .await
        .unwrap_or_else(|e| {
            tracing::error!("admin/assume: live session lookup failed: {e}");
            Vec::new()
        })
}

/// **Acting closes the admin surface.**
///
/// Without this, "act as" was decoration: you got a banner, kept every staff
/// power, and stayed on the admin page — so nothing about your reach actually
/// changed, and the mode had no meaning. Impersonation has to be a *place you go*,
/// not a hat you wear while continuing to be yourself.
///
/// It also removes a genuine hazard: staff writes performed through the admin API
/// during an assume window would be attributable to the real actor, but sit inside
/// an audit span that says "was acting as tenant X" — mixing two authorities in
/// one session is exactly the thing the audit chain is supposed to keep separate.
///
/// The `/assume` subtree is deliberately NOT behind this layer (see `router()`),
/// so the one-click exit always works. Ending the session restores admin.
pub async fn block_admin_while_acting(
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, StatusCode> {
    let Ok(db) = establish_connection().await else {
        // Can't tell ⇒ don't invent a lockout. Admin routes have their own guards.
        return Ok(next.run(request).await);
    };
    if !live_sessions_for(&db, actor.id).await.is_empty() {
        tracing::info!(
            actor = %actor.email,
            "admin/assume: admin surface refused — actor is currently acting as a tenant"
        );
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound is a hard ceiling, and it is NOT renewable — a longer
    /// investigation must leave a trail of deliberate re-entries.
    #[test]
    fn session_is_bounded_to_an_hour() {
        assert_eq!(MAX_SESSION, 60);
        let start = Utc::now();
        let expires = start + Duration::minutes(MAX_SESSION);
        assert!((expires - start).num_minutes() <= 60);
    }

    /// An empty reason is rejected before any row is written — the reason is the
    /// point, not decoration.
    #[test]
    fn blank_reason_is_rejected() {
        for raw in ["", "   ", "\t\n"] {
            assert!(raw.trim().is_empty(), "{raw:?} must be treated as blank");
        }
    }

    /// Liveness = not ended AND not expired. An expired-but-unended row grants
    /// nothing, so a forgotten session can't become a permanent backdoor.
    #[test]
    fn expiry_alone_kills_a_session() {
        let now = Utc::now();
        let expired = now - Duration::minutes(1);
        let live = now + Duration::minutes(1);
        assert!(expired < now, "an expired session must not be live");
        assert!(live > now, "an unexpired session is live");
    }
}
