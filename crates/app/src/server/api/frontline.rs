//! Frontline sign-in — the HTTP surface a kiosk talks to.
//!
//! `oxy_auth::frontline` decides whether a PIN is right. This decides what that
//! is worth: a session, scoped and short, that lets a worker be somebody for
//! the length of a shift.
//!
//! # Why these routes are public, and what stands in for auth
//!
//! A worker has nothing to authenticate WITH until they have signed in, so both
//! routes below sit in the public router. That makes rate limiting load-bearing
//! rather than defensive, and it is layered:
//!
//! * the credential itself throttles and locks out per `(org, identifier)`
//!   ([`oxy_auth::frontline::verify_pin`]);
//! * this module throttles per **org** as well, because the credential-level
//!   lockout is per worker and a caller walking a roster of 40 names gets 40
//!   separate budgets;
//! * every failure returns one response, so neither layer leaks which of the
//!   two refused.
//!
//! # Fleet role
//!
//! Both routes are `route_fleet`, and must be. They read and write Postgres and
//! touch no working copy, no `.git` and no state dir — and more to the point,
//! **signing in has to survive the ide restarting**. Pinning login to the
//! singleton would mean a deploy locks every store out of its own checklists.

use crate::server::api::middlewares::role_guards::OrgAdmin;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, http::header};
use entity::{org_frontline_members, organizations, user_credentials, users};
use oxy::database::client::establish_connection;
use oxy_app_core::audit;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use oxy_auth::frontline::{self, KIND_PIN, PinPolicy, PinVerdict};
use oxy_shared::errors::OxyError;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

/// How long a shift session lasts.
///
/// Twelve hours, not the week a magic-link session gets: this credential was
/// proved by four digits typed on a shared tablet in a room full of people, and
/// the tablet does not leave the store. A closing shift is the long case.
const SHIFT_HOURS: i64 = 12;

/// Per-org attempt ceiling within [`ORG_WINDOW`].
///
/// The credential's own lockout is per worker, so a caller trying `0000` once
/// against each of 40 names never trips it — 40 identifiers, one attempt each.
/// This is the ceiling that notices the *pattern* rather than the account.
const ORG_ATTEMPT_CEILING: usize = 30;
const ORG_WINDOW: Duration = Duration::from_secs(60);

/// Failed attempts per org, newest last. In-process on purpose: this is a
/// coarse brake in front of the real per-credential throttle, not the throttle
/// itself, so a per-replica window is the right cost. Making it shared state
/// would put a Postgres write on every wrong keypress to slow down an attacker
/// the credential layer already locks out.
static ORG_ATTEMPTS: LazyLock<Mutex<HashMap<Uuid, Vec<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn org_is_rate_limited(org_id: Uuid) -> bool {
    // A poisoned lock means a previous holder panicked. RECOVER rather than
    // fail closed: poisoning is permanent, so `Err(_) => return true` refused
    // every sign-in for every org for the life of the process — a self-inflicted
    // outage far larger than the guessing it was meant to stop. The guarded data
    // is a map of timestamps with no invariant a panic could leave broken, so
    // there is nothing to protect by refusing to read it.
    let mut map = match ORG_ATTEMPTS.lock() {
        Ok(m) => m,
        Err(poisoned) => poisoned.into_inner(),
    };
    let now = Instant::now();
    let hits = map.entry(org_id).or_default();
    hits.retain(|t| now.duration_since(*t) < ORG_WINDOW);
    hits.len() >= ORG_ATTEMPT_CEILING
}

fn record_org_attempt(org_id: Uuid) {
    let mut map = match ORG_ATTEMPTS.lock() {
        Ok(m) => m,
        Err(poisoned) => poisoned.into_inner(),
    };
    map.entry(org_id).or_default().push(Instant::now());
}

/// The throttle key for a request that named an org that does not exist.
///
/// There is no org id to key on, so key on the absence of one: a caller walking
/// slugs is one caller, and this bucket has no legitimate traffic to starve —
/// the worst it costs a real user is a `429` on a typo'd slug.
const NO_SUCH_ORG: Uuid = Uuid::nil();

#[derive(Debug, Deserialize)]
pub struct RosterQuery {
    /// Org slug — the kiosk knows which store it is bolted to.
    pub org: String,
}

#[derive(Debug, Serialize)]
pub struct RosterEntry {
    /// The stable login name the kiosk sends back with the PIN. Not the
    /// display name: renaming a worker must not change how they sign in.
    pub identifier: String,
    pub name: String,
}

/// The name picker.
///
/// Deliberately carries **no** credential material and **no** lockout state. It
/// is rendered on a screen anyone in the building can see, so it must not help
/// an attacker choose a target — "this one is locked out" would confirm both
/// that the worker exists and that somebody has been guessing at them.
///
/// It does leak the roster of one store to anyone who knows the org slug. That
/// is a deliberate trade and the reason the PIN is not the only control: the
/// tablet is on the wall, the names are on the schedule beside it, and a name
/// picker nobody can load is a kiosk nobody can use.
#[instrument(skip_all, fields(org = %q.org))]
pub async fn roster(Query(q): Query<RosterQuery>) -> impl IntoResponse {
    let Ok(db) = establish_connection().await else {
        // `{"staff": []}`, not `{}` — a kiosk reads `body.staff` and would get
        // `undefined` from the bare object, which is a render crash rather than
        // an empty picker.
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "staff": [] })),
        )
            .into_response();
    };

    let Ok(Some(org)) = organizations::Entity::find()
        .filter(organizations::Column::Slug.eq(&q.org))
        .one(&db)
        .await
    else {
        // An unknown org answers as an EMPTY roster, not a 404. A 404 here is
        // an org-slug oracle, and slugs are guessable.
        return Json(serde_json::json!({ "staff": [] })).into_response();
    };

    let rows = user_credentials::Entity::find()
        .filter(user_credentials::Column::Kind.eq(KIND_PIN))
        .filter(user_credentials::Column::OrgId.eq(Some(org.id)))
        .order_by_asc(user_credentials::Column::Identifier)
        // A roster is a screen, not a dataset. The cap is what stops a large
        // tenant turning the picker into a slow query on every kiosk load.
        .limit(200)
        .all(&db)
        .await
        .unwrap_or_default();

    // Names come from `users`, and only for workers whose standing is active —
    // a suspended worker must not appear on the picker at all.
    //
    // Two batched queries, not two per credential. The `.limit(200)` above caps
    // the ROW count, not the QUERY count, and this route is public, unthrottled
    // (`org_is_rate_limited` guards `login` only) and answers for any guessable
    // slug — so the per-row shape made a trivial loop a 400x amplifier against
    // the shared pool.
    let ids: Vec<Uuid> = rows.iter().map(|c| c.user_id).collect();
    let active: std::collections::HashSet<Uuid> = org_frontline_members::Entity::find()
        .filter(org_frontline_members::Column::OrgId.eq(org.id))
        .filter(org_frontline_members::Column::UserId.is_in(ids.clone()))
        .filter(org_frontline_members::Column::Status.eq("active"))
        .all(&db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.user_id)
        .collect();
    let names: std::collections::HashMap<Uuid, String> = users::Entity::find()
        .filter(users::Column::Id.is_in(ids))
        .all(&db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|u| (u.id, u.name))
        .collect();

    // Built from `rows` so the `identifier` sort the query asked for survives —
    // a picker whose order changes between loads is a picker people mis-tap.
    let staff: Vec<RosterEntry> = rows
        .into_iter()
        .filter(|c| active.contains(&c.user_id))
        .filter_map(|c| {
            names.get(&c.user_id).map(|name| RosterEntry {
                identifier: c.identifier,
                name: name.clone(),
            })
        })
        .collect();

    Json(serde_json::json!({ "staff": staff })).into_response()
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub org: String,
    pub identifier: String,
    pub pin: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub name: String,
    /// Seconds until the session expires — so a kiosk can show "signs out at
    /// 23:00" rather than discovering it mid-submission.
    pub expires_in: i64,
}

/// The `Set-Cookie` for a shift session.
///
/// Lifted out of `login` so the handler stays near the ~30-line guidance in
/// `crates/app/CLAUDE.md`, and because both decisions below are ones a reader
/// needs to see together rather than buried in a response builder.
fn shift_session_headers(token: &str, req_headers: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    // The same session cookie the web app uses, so an installed PWA carries it
    // on navigation without the page having to hold the token itself — and built
    // from the same two decisions the magic-link path makes, rather than by a
    // second rule that happens to agree in production.
    //
    // `Secure` from the REQUEST, not from the serve mode. A dev box is cloud
    // mode with non-prod secrets served over plain `http://localhost`, so
    // `!process_is_local()` set `Secure`, the browser discarded the cookie, and
    // kiosk sign-in returned 200 without sticking while magic-link login on the
    // same box worked. `is_request_secure` also honours `X-Forwarded-Proto`,
    // which matters behind an ingress terminating TLS with neither env var set.
    //
    // Max-Age from SHIFT_HOURS, not the 7-day default. The shift TTL was only
    // enforced by the JWT `exp`, so the browser kept a dead cookie for another
    // six days — the morning after a shift the kiosk looked signed in and 401'd
    // on every call instead of showing the name picker.
    let secure = super::auth::is_request_secure(req_headers);
    if let Ok(v) = header::HeaderValue::from_str(&super::auth::build_session_cookie_with_max_age(
        token,
        secure,
        SHIFT_HOURS * 3600,
    )) {
        out.insert(header::SET_COOKIE, v);
    }
    out
}

/// Exchange a PIN for a shift session.
///
/// The session is an ordinary Oxy JWT with a 12-hour expiry. It is narrow not
/// because the token says so but because the AUTHZ MODEL makes it narrow: a
/// frontline worker holds `org_frontline_members` standing, which
/// `oxy_authz::Ring::AppAccess` reads only when ANDed with an explicit
/// `app_members` grant. They reach the apps they were enrolled to use and
/// nothing else — no org read, no workspace, no settings.
///
/// That is worth stating because the alternative was tempting: a bespoke
/// token type, with a bespoke validation path, and a second place for an
/// authorization bug to live.
#[instrument(skip_all, fields(org = %body.org, identifier = %body.identifier))]
pub async fn login(req_headers: HeaderMap, body: Json<LoginRequest>) -> impl IntoResponse {
    let Json(body) = body;

    let Ok(db) = establish_connection().await else {
        return refuse(StatusCode::SERVICE_UNAVAILABLE);
    };

    let Ok(Some(org)) = organizations::Entity::find()
        .filter(organizations::Column::Slug.eq(&body.org))
        .one(&db)
        .await
    else {
        // THE BRAKE COMES FIRST. Closing the timing channel means paying the
        // Argon2 cost on this branch too — `Argon2::default()` is RFC 9106's
        // second profile, m = 19456 KiB and t = 2 — and this is the one path
        // through `login` that `org_is_rate_limited` below cannot cover, because
        // it has no org id to key on. Burning unmetered here would trade a
        // timing oracle for something strictly worse: ~19 MiB and two Argon2
        // passes per request, unauthenticated, at whatever concurrency the
        // caller picks. So the unknown-slug path gets its own bucket, and ends
        // up with exactly the same cost profile as the known-slug path.
        if org_is_rate_limited(NO_SUCH_ORG) {
            return refuse(StatusCode::TOO_MANY_REQUESTS);
        }
        record_org_attempt(NO_SUCH_ORG);

        // Same refusal as a wrong PIN, and the same COST. Matching bodies is
        // only half of it: a known slug goes on to `verify_pin`, which always
        // pays the verify, so returning after one indexed SELECT left the two
        // branches an order of magnitude apart in latency.
        //
        // Kept even though `roster` already discloses org existence by content
        // (`{"staff": []}` for an unknown slug, a list for a known one), so what
        // this closes today is only "org exists but has no frontline staff" vs
        // "no such org". It stays because that disclosure is a property of
        // `roster`, not a decision made here: gate `roster` behind a device
        // token later and this branch would silently become an oracle again.
        oxy_auth::frontline::burn_verify_time(&body.pin);
        return refuse(StatusCode::UNAUTHORIZED);
    };

    if org_is_rate_limited(org.id) {
        warn!(org_id = %org.id, "frontline login rate-limited for this org");
        // 429 rather than 401: this one IS worth telling the caller apart,
        // because a kiosk should back off rather than retry, and the fact
        // leaked ("somebody is guessing at this org") is not the roster.
        return refuse(StatusCode::TOO_MANY_REQUESTS);
    }

    let verdict = match frontline::verify_pin(
        &db,
        org.id,
        &body.identifier,
        &body.pin,
        PinPolicy::default(),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "frontline verify failed");
            return refuse(StatusCode::SERVICE_UNAVAILABLE);
        }
    };

    let PinVerdict::Ok { user_id } = verdict else {
        record_org_attempt(org.id);
        // One response for every failure — wrong PIN, locked out, no such
        // worker, malformed. `PinVerdict::public_message` exists for exactly
        // this and the difference stays in the log.
        info!(verdict = ?verdict, "frontline login refused");
        return refuse(StatusCode::UNAUTHORIZED);
    };

    let Ok(Some(user)) = users::Entity::find_by_id(user_id).one(&db).await else {
        return refuse(StatusCode::UNAUTHORIZED);
    };
    let name = user.name.clone();

    let token =
        match super::auth::create_auth_token_with_ttl(user, chrono::Duration::hours(SHIFT_HOURS))
            .await
        {
            Ok(t) => t,
            Err(status) => return refuse(status),
        };

    info!(%user_id, org_id = %org.id, "frontline session opened");

    let out_headers = shift_session_headers(&token, &req_headers);

    (
        out_headers,
        Json(LoginResponse {
            token,
            name,
            expires_in: SHIFT_HOURS * 3600,
        }),
    )
        .into_response()
}

/// Every refusal, in one shape.
fn refuse(status: StatusCode) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({ "error": "that PIN did not match" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cookie has to die with the token it carries.
    ///
    /// It did not: `build_session_cookie` hardcoded seven days while the shift
    /// JWT expires in twelve hours, so the browser kept presenting a dead
    /// credential for another six days. That does not read as "signed out" — the
    /// kiosk looks signed in and 401s on every call.
    #[test]
    fn the_session_cookie_expires_with_the_shift() {
        let cookie =
            super::super::auth::build_session_cookie_with_max_age("tok", true, SHIFT_HOURS * 3600);
        assert!(
            cookie.contains(&format!("Max-Age={}", SHIFT_HOURS * 3600)),
            "cookie outlives the token: {cookie}"
        );
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
    }

    /// `Secure` comes from the request, not the serve mode. A dev box is cloud
    /// mode over plain http, so a mode-derived flag set `Secure` there and the
    /// browser silently dropped the cookie.
    #[test]
    fn an_insecure_request_gets_a_cookie_the_browser_will_keep() {
        let cookie = super::super::auth::build_session_cookie_with_max_age("tok", false, 3600);
        assert!(
            !cookie.contains("Secure"),
            "a plain-http kiosk would discard this: {cookie}"
        );
    }

    #[test]
    fn the_org_brake_opens_and_closes() {
        let org = Uuid::new_v4();
        assert!(!org_is_rate_limited(org), "a fresh org is not limited");
        for _ in 0..ORG_ATTEMPT_CEILING {
            record_org_attempt(org);
        }
        assert!(
            org_is_rate_limited(org),
            "the ceiling must actually stop a caller walking the roster — the \
             per-credential lockout cannot, because 40 names is 40 budgets"
        );
        // A different org is unaffected: the brake is per tenant, so one store
        // under attack cannot lock out another.
        assert!(!org_is_rate_limited(Uuid::new_v4()));
    }

    #[tokio::test]
    async fn every_refusal_carries_the_same_body() {
        // The status differs (429 tells a kiosk to back off) but the body must
        // not, or the response distinguishes "no such worker" from "wrong PIN".
        //
        // READ the body. The first version of this test `Debug`-formatted an
        // unread `axum::body::Body`, which prints the same opaque placeholder
        // for every response — so it compared three identical strings and could
        // not fail. It would have passed with three different bodies, which is
        // the only thing it was written to catch.
        let mut bodies = Vec::new();
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            let bytes = axum::body::to_bytes(refuse(status).into_body(), 64 * 1024)
                .await
                .expect("refusal bodies are small and always present");
            bodies.push(String::from_utf8(bytes.to_vec()).expect("utf-8"));
        }
        assert!(
            !bodies[0].is_empty(),
            "a refusal with an empty body would make this vacuous again"
        );
        assert!(
            bodies.windows(2).all(|w| w[0] == w[1]),
            "refusals differ and leak which layer refused: {bodies:?}"
        );
    }

    /// The guard on the test above: prove the comparison can actually fail.
    ///
    /// Without this, a future edit that reverts `refuse` to something opaque
    /// makes the assertion vacuous again with nothing to notice.
    #[tokio::test]
    async fn the_refusal_body_comparison_can_fail() {
        let a = axum::body::to_bytes(refuse(StatusCode::UNAUTHORIZED).into_body(), 64 * 1024)
            .await
            .unwrap();
        let b = axum::body::to_bytes(
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({"x": 1})))
                .into_response()
                .into_body(),
            64 * 1024,
        )
        .await
        .unwrap();
        assert_ne!(a, b, "two genuinely different bodies compared equal");
    }
}

// ── Enrolment ───────────────────────────────────────────────────────────────

/// No `Debug`. This struct holds a raw PIN, and a derived `Debug` is one
/// `tracing` field or one `unwrap` panic away from putting it in a log line.
#[derive(Deserialize)]
pub struct EnrolRequest {
    /// Shown on the kiosk's name picker. Not unique — two Marias are two rows.
    pub name: String,
    /// What the worker picks themselves out by. Unique per org.
    pub identifier: String,
    /// 4–8 digits. Never stored, never logged, never returned.
    pub pin: String,
}

/// Enrol a frontline worker — the door `enroll_worker` never had.
///
/// # Why this is the missing piece
///
/// Everything else in this file already shipped: the PIN credential, the
/// standing row, the login exchange, the roster read. `oxy_auth::frontline::
/// enroll_worker` has existed the whole time with **zero non-test callers**, so
/// `GET /api/frontline/roster` has been answering `200 {"staff": []}` on every
/// deployment — a read path with no write path, which looks exactly like a
/// tenant that has not enrolled anybody.
///
/// # Who may call it
///
/// `OrgAdmin`, which is the guard the rest of the org's member management uses.
/// Deliberately NOT a new authorization concept: enrolling a worker is adding a
/// person to an org, and inventing a second rule for it is how two answers to
/// one question start disagreeing. A store manager who is not an org admin
/// cannot enrol yet — that is a real gap, and it is one for the roles model to
/// close rather than for this route to route around.
///
/// # What it deliberately does not do
///
/// No email, no invitation, no `org_members` row. That is the whole design:
/// `enroll_worker` writes `users.email = NULL`, which keeps this person out of
/// every email-keyed path — OAuth collapse, Slack matching, invitations,
/// platform grants — by construction rather than by a check somebody has to
/// remember. The worker exists, can sign in, and holds nothing else.
#[instrument(skip_all, fields(org = %org_id))]
pub async fn enrol(
    OrgAdmin(_ctx): OrgAdmin,
    Path(org_id): Path<Uuid>,
    Json(req): Json<EnrolRequest>,
) -> impl IntoResponse {
    let Ok(db) = establish_connection().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "database unavailable" })),
        )
            .into_response();
    };

    match frontline::enroll_worker(
        &db,
        org_id,
        &req.name,
        &req.identifier,
        &req.pin,
        PinPolicy::default(),
    )
    .await
    {
        Ok(user_id) => {
            // The PIN is not echoed. An admin who did not keep it re-enrols or
            // resets; a response that repeats it would put it in every proxy
            // log between here and the browser.
            info!(%org_id, %user_id, "frontline worker enrolled");
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "user_id": user_id,
                    "identifier": req.identifier.trim(),
                    "name": req.name.trim(),
                })),
            )
                .into_response()
        }
        // Match the VARIANT, not every error.
        //
        // `enroll_worker` validates the PIN policy and the required fields, and
        // those messages are exactly what an admin needs — so a validation
        // failure is a 400 carrying its own sentence.
        //
        // Everything else is ours. Mapping them all to 400 told an admin that a
        // pool exhaustion was their bad request, and put raw database text
        // ("enrol begin: …") in the response body on the way. A 500 with a
        // generic body is the honest answer; the real error goes to the log,
        // where it belongs.
        Err(OxyError::ValidationError(msg)) => {
            warn!(%org_id, "frontline enrolment refused: {msg}");
            // 409 for a taken identifier, 400 for a malformed request.
            //
            // Re-enrolling somebody who already exists, or reusing a badge
            // number, is the most likely way this call fails and is entirely
            // the admin's to fix — it is a conflict with existing state, not a
            // bad request. Matched on `frontline::IDENTIFIER_TAKEN` rather than
            // on a literal, so the two sides cannot drift apart silently.
            let status = if msg == frontline::IDENTIFIER_TAKEN {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(serde_json::json!({ "error": msg }))).into_response()
        }
        Err(e) => {
            error!(%org_id, error = %e, "frontline enrolment failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "could not enrol the worker" })),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct StandingRequest {
    /// `false` suspends, `true` reinstates.
    pub active: bool,
}

/// Suspend a frontline worker, or reinstate one.
///
/// The other half of enrolment, and it was missing: a worker could be enrolled
/// and never un-enrolled. The `status` column has modelled `suspended` since the
/// schema landed and nothing wrote it, so the door opened one way — which I
/// found by enrolling a test worker into a demo org and having no way to remove
/// them.
///
/// `PATCH`, not `DELETE`, because nothing is deleted. A worker who leaves keeps
/// their row so the work they did stays attributed; suspension is what takes
/// away the ability to sign in. `verify_pin` and the roster read the same
/// column, so one write closes the door on both the login and the name picker.
///
/// Idempotent. Suspending an already-suspended worker answers 200 with
/// `changed: false` rather than 409 — the caller asked for a state and that is
/// the state, and making a retry look like a conflict is how a client learns to
/// ignore the status code.
#[instrument(skip_all, fields(org = %org_id, worker = %user_id))]
pub async fn set_standing(
    OrgAdmin(ctx): OrgAdmin,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((org_id, user_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<StandingRequest>,
) -> impl IntoResponse {
    let Ok(db) = establish_connection().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "database unavailable" })),
        )
            .into_response();
    };

    match frontline::set_worker_standing(&db, org_id, user_id, req.active).await {
        Ok(changed) => {
            // "Who cut this worker off, and when."
            //
            // The neighbouring route in this same router writes
            // `org.member.removed` under a comment reading "Losing access is as
            // auditable as gaining it" — and suspension IS losing access. This
            // route shipped with a `tracing::info!` carrying no actor at all, so
            // once logs rolled the answer was unrecoverable.
            //
            // Only on a real change: an idempotent no-op is not an event, and an
            // audit log that records every retry is one nobody reads.
            //
            // Best-effort, like its neighbour: failing to write the trail must
            // not fail a revocation that has already happened. A worker whose
            // access was removed but whose removal went unlogged is bad; leaving
            // their access in place because the logging failed is worse.
            if changed {
                // `user_can_access_app` caches its verdict per (user, app) for
                // the app shell and every function invoke; a suspension that
                // left that entry warm would keep the kiosk working until it
                // expired. Same call every grant-changing route makes.
                crate::server::api::custom_apps_auth::invalidate_access_cache();
                let action = if req.active {
                    "frontline.worker.reinstated"
                } else {
                    "frontline.worker.suspended"
                };
                audit::record_best_effort(
                    &db,
                    audit::AuditEntry::new(actor.label().to_string(), action)
                        .actor(actor.id, audit::ActorType::User)
                        .org(ctx.org.id)
                        .target("frontline_worker", user_id.to_string(), String::new())
                        .change(
                            serde_json::json!({ "active": !req.active }),
                            serde_json::json!({ "active": req.active }),
                        ),
                )
                .await;
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "user_id": user_id,
                    "active": req.active,
                    // What the statement DID, not what was asked for. The two differ
                    // exactly when the worker was already in that state, and a
                    // caller reconciling a roster needs to tell those apart.
                    "changed": changed,
                })),
            )
                .into_response()
        }
        // Same split as enrolment: the admin's mistake carries its sentence,
        // everything else is ours and says nothing about the database.
        Err(OxyError::ValidationError(msg)) => {
            warn!(%org_id, "frontline standing refused: {msg}");
            // 404 for THIS refusal, matched by name — 400 for any other.
            //
            // Blanket-mapping the variant was right while the writer had exactly
            // one validation, and would have quietly reported the next one — a
            // bad status, a self-suspend guard — as "worker not found". Same
            // reasoning as `enrol` matching `IDENTIFIER_TAKEN` rather than a
            // literal: the two sides agree on a name so they cannot drift.
            let status = if msg == frontline::WORKER_NOT_FOUND {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(serde_json::json!({ "error": msg }))).into_response()
        }
        Err(e) => {
            error!(%org_id, error = %e, "frontline standing failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "could not change the worker's standing" })),
            )
                .into_response()
        }
    }
}
