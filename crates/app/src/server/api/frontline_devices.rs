//! Kiosk devices: the binding a frontline PIN is only ever usable inside.
//!
//! # The threat, and the shape of the answer
//!
//! A PIN is four to six digits. `POST /api/frontline/login` verified one for
//! any client that could reach the route, so the whole tenant's crew was a
//! brute-force target with a five-digit keyspace, bounded per identifier by the
//! credential lockout and per org by a rate limit — and by nothing else. The
//! design record names device binding as required before this faces a user.
//!
//! This is that binding. An org admin **creates** a device and gets a one-time
//! enrol link; opening it on the tablet **binds** the device, which sets a
//! long-lived HttpOnly cookie (`oxy_kiosk`) holding a random secret. From then
//! on `login` and `roster` answer only requests carrying a cookie that resolves
//! to an unrevoked device **of the same org**: a PIN typed anywhere else is
//! refused with the same 401 as a wrong PIN, so an attacker learns nothing
//! about whether the identifier exists. **Revoke** is a timestamp; the row is
//! the audit trail of which tablet a shift was signed in on.
//!
//! # What this is not
//!
//! Not a second session. The device cookie says *where* a PIN may be entered;
//! the PIN says *who*. A signed-in worker's session is the ordinary session
//! cookie `login` mints, and everything downstream (`user_can_access_app`,
//! the rings) sees a user, not a device. Not device attestation either: a
//! cookie can be copied off a tablet by someone with the tablet. What it
//! closes is the network — a PIN is no longer a bearer credential.
//!
//! # Where things live
//!
//! Secrets are hashed with SHA-256 — they are 256 random bits, so the slow hash
//! the PIN needs (`oxy_auth::frontline::hash_pin`) would buy nothing here and
//! would put an Argon2 pass on every roster read. The pure functions
//! (`create`, `bind_with_token`, `revoke`, `bound_device`) take a database and
//! are what `tests/platform/frontline_devices.rs` drives; the handlers are thin.

use axum::Json;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use entity::{org_kiosk_devices as devices, organizations};
use oxy::database::client::establish_connection;
use oxy_app_core::audit;
use oxy_auth::extractor::AuthenticatedUserExtractor;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::server::api::auth::{
    extract_base_url_from_headers, is_request_secure, validate_return_to_url,
};
use crate::server::api::middlewares::role_guards::OrgAdmin;

/// The cookie an enrolled kiosk carries: `<device id>.<secret>`.
pub const KIOSK_COOKIE_NAME: &str = "oxy_kiosk";
/// An enrol link is good for a day. Long enough to walk the tablet to the
/// counter; short enough that a link left in a chat thread is dead by the time
/// anyone finds it.
const ENROL_LINK_HOURS: i64 = 24;
/// A bound device stays bound for a year of calendar time; a lost tablet is
/// handled by revocation, not by expiry.
const DEVICE_COOKIE_MAX_AGE_SECS: i64 = 365 * 24 * 60 * 60;
const NAME_MAX_CHARS: usize = 80;

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// 244 bits from the OS, as 64 hex characters. Two v4 UUIDs rather than a
/// `rand` call: the crate is a workspace dependency whose API moved between
/// majors, and `getrandom` underneath `Uuid::new_v4` is the same source.
fn random_secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Constant-time equality over two hex digests. The digests are public-shaped
/// but the comparison must not leak how many leading bytes matched.
fn digest_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// The `oxy_kiosk` value, if the request carries one. Same RFC 6265 split as
/// `oxy_auth::built_in::extract_session_cookie`, for the same reason it exists
/// there: callers drifted on the empty-value guard.
pub fn extract_kiosk_cookie(headers: &HeaderMap) -> Option<String> {
    let prefix = format!("{KIOSK_COOKIE_NAME}=");
    for value in headers.get_all(header::COOKIE).iter() {
        let Ok(raw) = value.to_str() else { continue };
        for part in raw.split(';') {
            if let Some(v) = part.trim().strip_prefix(prefix.as_str())
                && !v.is_empty()
            {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// An enrolled, unrevoked kiosk the request proved it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundDevice {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub return_to: Option<String>,
}

/// Resolve the device a request's kiosk cookie names — bound, unrevoked, and
/// holding the secret the cookie carries. `None` for every other case, and
/// deliberately one `None`: the caller answers the same way whether the cookie
/// is absent, forged, revoked or stale.
pub async fn bound_device(db: &DatabaseConnection, headers: &HeaderMap) -> Option<BoundDevice> {
    let raw = extract_kiosk_cookie(headers)?;
    let (id, secret) = raw.split_once('.')?;
    let id = Uuid::parse_str(id).ok()?;
    let row = devices::Entity::find_by_id(id)
        .one(db)
        .await
        .ok()
        .flatten()?;
    if row.revoked_at.is_some() || row.bound_at.is_none() {
        return None;
    }
    let expected = row.secret_hash.as_deref()?;
    if !digest_eq(expected, &sha256_hex(secret)) {
        return None;
    }
    Some(BoundDevice {
        id: row.id,
        org_id: row.org_id,
        name: row.name,
        return_to: row.return_to,
    })
}

/// Stamp `last_seen_at`. Best-effort and only on a sign-in, not on every roster
/// read: "when was this tablet last used" is a question about shifts.
pub async fn touch(db: &DatabaseConnection, device_id: Uuid) {
    let am = devices::ActiveModel {
        id: Set(device_id),
        last_seen_at: Set(Some(Utc::now().into())),
        ..Default::default()
    };
    if let Err(e) = am.update(db).await {
        warn!(device = %device_id, error = %e, "kiosk last_seen_at not stamped");
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("a device needs a name of 1 to {NAME_MAX_CHARS} characters")]
    BadName,
    #[error("return_to is not a destination this deployment allows")]
    BadReturnTo,
    /// Covers expired, already used, revoked and never issued — one arm, so the
    /// public bind route cannot be used to tell those apart.
    #[error("that enrol link is not valid")]
    NoSuchToken,
    #[error("no such device")]
    NotFound,
    #[error("database error: {0}")]
    Db(#[from] DbErr),
}

/// Create a device and mint its one-time enrol token. The token is returned
/// exactly once, here; only its hash is stored.
pub async fn create(
    db: &DatabaseConnection,
    org_id: Uuid,
    name: &str,
    return_to: Option<&str>,
    created_by: Option<Uuid>,
) -> Result<(devices::Model, String), DeviceError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
        return Err(DeviceError::BadName);
    }
    let return_to = match return_to.map(str::trim).filter(|s| !s.is_empty()) {
        None => None,
        Some(url) if validate_return_to_url(url) => Some(url.to_string()),
        Some(_) => return Err(DeviceError::BadReturnTo),
    };
    let token = random_secret();
    let now = Utc::now();
    let row = devices::ActiveModel {
        id: Set(Uuid::new_v4()),
        org_id: Set(org_id),
        name: Set(name.to_string()),
        return_to: Set(return_to),
        enrol_token_hash: Set(Some(sha256_hex(&token))),
        enrol_expires_at: Set(Some((now + Duration::hours(ENROL_LINK_HOURS)).into())),
        secret_hash: Set(None),
        created_by: Set(created_by),
        created_at: Set(now.into()),
        bound_at: Set(None),
        last_seen_at: Set(None),
        revoked_at: Set(None),
    }
    .insert(db)
    .await?;
    Ok((row, token))
}

/// Trade a one-time enrol token for the device secret. Returns the row and
/// the cookie VALUE (`<id>.<secret>`); the secret is never stored, only hashed.
///
/// Single use by construction: the token hash is cleared in the same UPDATE
/// that sets the secret, and the UPDATE is filtered on the hash still being
/// present, so two tablets opening the same link race on one row and exactly
/// one of them binds.
pub async fn bind_with_token(
    db: &DatabaseConnection,
    token: &str,
) -> Result<(devices::Model, String), DeviceError> {
    let hash = sha256_hex(token.trim());
    let now = Utc::now();
    let row = devices::Entity::find()
        .filter(devices::Column::EnrolTokenHash.eq(&hash))
        .filter(devices::Column::RevokedAt.is_null())
        .filter(devices::Column::BoundAt.is_null())
        .one(db)
        .await?
        .ok_or(DeviceError::NoSuchToken)?;
    if row.enrol_expires_at.is_none_or(|t| t < now) {
        return Err(DeviceError::NoSuchToken);
    }
    let secret = random_secret();
    let res = devices::Entity::update_many()
        .col_expr(
            devices::Column::SecretHash,
            sea_orm::sea_query::Expr::value(sha256_hex(&secret)),
        )
        .col_expr(
            devices::Column::BoundAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .col_expr(
            devices::Column::LastSeenAt,
            sea_orm::sea_query::Expr::value(now),
        )
        .col_expr(
            devices::Column::EnrolTokenHash,
            sea_orm::sea_query::Expr::value(Option::<String>::None),
        )
        .col_expr(
            devices::Column::EnrolExpiresAt,
            sea_orm::sea_query::Expr::value(Option::<chrono::DateTime<chrono::FixedOffset>>::None),
        )
        .filter(devices::Column::Id.eq(row.id))
        .filter(devices::Column::EnrolTokenHash.eq(&hash))
        .exec(db)
        .await?;
    if res.rows_affected != 1 {
        return Err(DeviceError::NoSuchToken);
    }
    let bound = devices::Entity::find_by_id(row.id)
        .one(db)
        .await?
        .ok_or(DeviceError::NotFound)?;
    let cookie_value = format!("{}.{secret}", bound.id);
    Ok((bound, cookie_value))
}

/// Switch a device off. `Ok(true)` when this call did it, `Ok(false)` when it
/// was already off; `NotFound` when the org has no such device — the org
/// filter is what keeps one tenant from revoking another's tablets.
pub async fn revoke(db: &DatabaseConnection, org_id: Uuid, id: Uuid) -> Result<bool, DeviceError> {
    let row = devices::Entity::find_by_id(id)
        .filter(devices::Column::OrgId.eq(org_id))
        .one(db)
        .await?
        .ok_or(DeviceError::NotFound)?;
    if row.revoked_at.is_some() {
        return Ok(false);
    }
    let res = devices::Entity::update_many()
        .col_expr(
            devices::Column::RevokedAt,
            sea_orm::sea_query::Expr::value(Utc::now()),
        )
        .filter(devices::Column::Id.eq(id))
        .filter(devices::Column::RevokedAt.is_null())
        .exec(db)
        .await?;
    Ok(res.rows_affected == 1)
}

fn kiosk_cookie(value: &str, secure: bool) -> String {
    let mut parts = vec![
        format!("{KIOSK_COOKIE_NAME}={value}"),
        "Path=/".to_string(),
        format!("Max-Age={DEVICE_COOKIE_MAX_AGE_SECS}"),
        "HttpOnly".to_string(),
        "SameSite=Lax".to_string(),
    ];
    if secure {
        parts.push("Secure".to_string());
    }
    // Same `Domain` rule as the session cookie, or the two would disagree on
    // which hosts a kiosk is a kiosk for.
    if let Ok(domain) = std::env::var("OXY_SESSION_COOKIE_DOMAIN") {
        let domain = domain.trim();
        if !domain.is_empty() {
            parts.push(format!("Domain={domain}"));
        }
    }
    parts.join("; ")
}

fn json_error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

/// A response whose body depends on the kiosk cookie must never be served from
/// a cache keyed on the URL alone: a shared cache in front of the fleet would
/// hand one kiosk's roster — or its org and device name — to an unbound
/// browser, or fill from an unbound request and blank the real kiosk. There is
/// no global cache layer on `/api`, so each cookie-gated handler says so itself.
pub fn no_store(resp: &mut Response) {
    let h = resp.headers_mut();
    h.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, private"),
    );
    h.insert(header::VARY, HeaderValue::from_static("Cookie"));
}

fn html(status: StatusCode, body: String) -> Response {
    let mut resp = (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response();
    no_store(&mut resp);
    resp
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const PAGE_CSS: &str = "body{font:16px/1.5 system-ui,sans-serif;max-width:32rem;\
margin:15vh auto;padding:0 1.5rem;color:#15191c}\
h1{font-size:1.4rem;margin:0 0 .5rem}p{color:#4d585f}\
button{font:inherit;font-size:1.1rem;padding:.9rem 1.4rem;border:0;border-radius:6px;\
background:#245a86;color:#fff;width:100%;margin-top:1.2rem}";

/// The tablet reaches the enrol routes by navigation, so even "the database is
/// away" has to be a page and not JSON.
fn unavailable_page() -> Response {
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        format!(
            "<!doctype html><meta name=viewport content=\"width=device-width\">\
             <meta name=\"referrer\" content=\"no-referrer\">\
             <title>Enrol kiosk</title><style>{PAGE_CSS}</style>\
             <h1>Enrolment is not available right now.</h1>\
             <p>Nothing was changed. Try the link again in a moment.</p>"
        ),
    )
}

fn dead_link_page() -> Response {
    html(
        StatusCode::NOT_FOUND,
        format!(
            "<!doctype html><meta name=viewport content=\"width=device-width\">\
             <title>Kiosk link</title><style>{PAGE_CSS}</style>\
             <h1>This kiosk link is not valid any more.</h1>\
             <p>It may have expired, or already been used. \
             Ask your manager for a new one.</p>"
        ),
    )
}

/// Look a token up WITHOUT spending it — what the confirm page needs to name
/// the device. Same filters as the bind, so the page and the bind agree on
/// which links are live.
pub async fn peek_token(
    db: &DatabaseConnection,
    token: &str,
) -> Result<devices::Model, DeviceError> {
    let row = devices::Entity::find()
        .filter(devices::Column::EnrolTokenHash.eq(sha256_hex(token.trim())))
        .filter(devices::Column::RevokedAt.is_null())
        .filter(devices::Column::BoundAt.is_null())
        .one(db)
        .await?
        .ok_or(DeviceError::NoSuchToken)?;
    if row.enrol_expires_at.is_none_or(|t| t < Utc::now()) {
        return Err(DeviceError::NoSuchToken);
    }
    Ok(row)
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `GET /api/frontline/device` — public. Tells the login page whether it is
/// running on an enrolled kiosk, and for which org, so it can offer crew
/// sign-in. Never an error: an unreadable cookie is "not a kiosk".
pub async fn device_status(headers: HeaderMap) -> Response {
    let mut resp = device_status_body(&headers).await;
    no_store(&mut resp);
    resp
}

async fn device_status_body(headers: &HeaderMap) -> Response {
    let unbound = || Json(serde_json::json!({ "bound": false })).into_response();
    let Ok(db) = establish_connection().await else {
        return unbound();
    };
    let Some(device) = bound_device(&db, headers).await else {
        return unbound();
    };
    let Ok(Some(org)) = organizations::Entity::find_by_id(device.org_id)
        .one(&db)
        .await
    else {
        return unbound();
    };
    Json(serde_json::json!({
        "bound": true,
        "org": org.slug,
        "orgName": org.name,
        "device": device.name,
        "returnTo": device.return_to,
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct BindQuery {
    pub token: String,
}

/// `GET /api/frontline/devices/bind?token=…` — public; the enrol link. Shows
/// a confirm page and binds NOTHING: the link travels through Slack, email or
/// SMS on its way to the tablet, and every one of those unfurls or scans URLs.
/// A GET that spent the single-use token would be consumed by the first bot to
/// look at it and the tablet would find a dead link. The binding happens on the
/// form's POST, which no unfurler submits (and which is what RFC 9110 says a
/// side effect needs anyway).
pub async fn bind_page(Query(q): Query<BindQuery>) -> Response {
    let Ok(db) = establish_connection().await else {
        return unavailable_page();
    };
    match peek_token(&db, &q.token).await {
        Ok(row) => html(
            StatusCode::OK,
            format!(
                "<!doctype html><meta name=viewport content=\"width=device-width\">\
                 <meta name=\"referrer\" content=\"no-referrer\">\
                 <title>Enrol kiosk</title><style>{PAGE_CSS}</style>\
                 <h1>Enrol this tablet as \u{201c}{name}\u{201d}?</h1>\
                 <p>Only do this on the device that will stay at the counter. \
                 It signs the crew in from here on.</p>\
                 <form method=\"post\" action=\"/api/frontline/devices/bind\">\
                 <input type=\"hidden\" name=\"token\" value=\"{token}\">\
                 <button type=\"submit\">Enrol this tablet</button></form>",
                name = escape(&row.name),
                token = escape(q.token.trim()),
            ),
        ),
        Err(DeviceError::NoSuchToken) => dead_link_page(),
        Err(e) => {
            warn!(error = %e, "kiosk enrol page failed");
            unavailable_page()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct BindForm {
    pub token: String,
}

/// `POST /api/frontline/devices/bind` (form body `token=…`) — public; the
/// confirm page's submit. Binds the device, sets the kiosk cookie, and sends
/// the tablet to the login page (with the app the admin named as `return_to`,
/// when there is one).
///
/// The token is spent by `bind_with_token` BEFORE the cookie is built, so a
/// cookie the header layer rejects — a stray character in
/// `OXY_SESSION_COOKIE_DOMAIN`, say — must not be silently dropped into a
/// redirect that looks like success: the link is dead by then and every retry
/// would look identical. It is logged and answered as the failure it is.
pub async fn bind_submit(headers: HeaderMap, body: String) -> Response {
    let token = match serde_urlencoded::from_str::<BindForm>(&body) {
        Ok(f) => f.token,
        Err(_) => return dead_link_page(),
    };
    let Ok(db) = establish_connection().await else {
        return unavailable_page();
    };
    match bind_with_token(&db, &token).await {
        Ok((row, cookie_value)) => {
            let cookie = kiosk_cookie(&cookie_value, is_request_secure(&headers));
            let Ok(cookie) = HeaderValue::from_str(&cookie) else {
                warn!(
                    device = %row.id,
                    "kiosk bound but its cookie header was rejected — check \
                     OXY_SESSION_COOKIE_DOMAIN; the enrol link is spent, create another device"
                );
                return html(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!(
                        "<!doctype html><meta name=viewport content=\"width=device-width\">\
                         <title>Enrol kiosk</title><style>{PAGE_CSS}</style>\
                         <h1>This tablet could not be enrolled.</h1>\
                         <p>The link has been used but the device could not be remembered on this \
                         browser. Ask your manager to create the kiosk again and to check the \
                         server's cookie settings.</p>"
                    ),
                );
            };
            info!(device = %row.id, org = %row.org_id, "kiosk bound");
            let to = match row.return_to.as_deref() {
                Some(url) => format!("/login?return_to={}", urlencoding::encode(url)),
                None => "/login".to_string(),
            };
            let mut resp = Redirect::to(&to).into_response();
            resp.headers_mut().insert(header::SET_COOKIE, cookie);
            no_store(&mut resp);
            resp
        }
        Err(DeviceError::NoSuchToken) => dead_link_page(),
        Err(e) => {
            warn!(error = %e, "kiosk bind failed");
            unavailable_page()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateDeviceRequest {
    pub name: String,
    #[serde(default)]
    pub return_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreatedDevice {
    pub id: Uuid,
    pub name: String,
    /// Open this on the tablet. Shown once; the server keeps only a hash.
    pub enrol_url: String,
    pub expires_at: String,
}

/// `POST /api/orgs/{org_id}/frontline/devices` — org admin. Creates a device
/// and answers with the one-time enrol link.
#[instrument(skip_all, fields(org = %org_id))]
pub async fn create_device(
    OrgAdmin(_ctx): OrgAdmin,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path(org_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CreateDeviceRequest>,
) -> Response {
    let Ok(db) = establish_connection().await else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "database unavailable");
    };
    match create(
        &db,
        org_id,
        &req.name,
        req.return_to.as_deref(),
        Some(actor.id),
    )
    .await
    {
        Ok((row, token)) => {
            let base = extract_base_url_from_headers(&headers);
            audit::record_best_effort(
                &db,
                audit::AuditEntry::new(actor.label().to_string(), "frontline.device.created")
                    .actor(actor.id, audit::ActorType::User)
                    .org(org_id)
                    .target("frontline_device", row.id.to_string(), row.name.clone()),
            )
            .await;
            (
                StatusCode::CREATED,
                Json(CreatedDevice {
                    id: row.id,
                    name: row.name,
                    enrol_url: format!("{base}/api/frontline/devices/bind?token={token}"),
                    expires_at: row
                        .enrol_expires_at
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_default(),
                }),
            )
                .into_response()
        }
        Err(e @ (DeviceError::BadName | DeviceError::BadReturnTo)) => {
            json_error(StatusCode::BAD_REQUEST, e.to_string())
        }
        Err(e) => {
            warn!(error = %e, "kiosk device create failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "create failed")
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DeviceRow {
    pub id: Uuid,
    pub name: String,
    pub return_to: Option<String>,
    pub created_at: String,
    pub bound_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
    /// Set while an enrol link is outstanding; the link itself is not
    /// recoverable — create another device if it was lost.
    pub enrol_expires_at: Option<String>,
}

/// `GET /api/orgs/{org_id}/frontline/devices` — org admin. Newest first; no
/// hash of anything leaves the server.
#[instrument(skip_all, fields(org = %org_id))]
pub async fn list_devices(OrgAdmin(_ctx): OrgAdmin, Path(org_id): Path<Uuid>) -> Response {
    let Ok(db) = establish_connection().await else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "database unavailable");
    };
    match devices::Entity::find()
        .filter(devices::Column::OrgId.eq(org_id))
        .order_by_desc(devices::Column::CreatedAt)
        .all(&db)
        .await
    {
        Ok(rows) => {
            let rfc = |t: Option<chrono::DateTime<chrono::FixedOffset>>| t.map(|t| t.to_rfc3339());
            let devices: Vec<DeviceRow> = rows
                .into_iter()
                .map(|r| DeviceRow {
                    id: r.id,
                    name: r.name,
                    return_to: r.return_to,
                    created_at: r.created_at.to_rfc3339(),
                    bound_at: rfc(r.bound_at),
                    last_seen_at: rfc(r.last_seen_at),
                    revoked_at: rfc(r.revoked_at),
                    enrol_expires_at: rfc(r.enrol_expires_at),
                })
                .collect();
            Json(serde_json::json!({ "devices": devices })).into_response()
        }
        Err(e) => {
            warn!(error = %e, "kiosk device list failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "list failed")
        }
    }
}

/// `DELETE /api/orgs/{org_id}/frontline/devices/{id}` — org admin. Revokes;
/// the row stays. A kiosk cookie for a revoked device answers like no cookie.
#[instrument(skip_all, fields(org = %org_id, device = %id))]
pub async fn revoke_device(
    OrgAdmin(_ctx): OrgAdmin,
    AuthenticatedUserExtractor(actor): AuthenticatedUserExtractor,
    Path((org_id, id)): Path<(Uuid, Uuid)>,
) -> Response {
    let Ok(db) = establish_connection().await else {
        return json_error(StatusCode::SERVICE_UNAVAILABLE, "database unavailable");
    };
    match revoke(&db, org_id, id).await {
        Ok(changed) => {
            if changed {
                audit::record_best_effort(
                    &db,
                    audit::AuditEntry::new(actor.label().to_string(), "frontline.device.revoked")
                        .actor(actor.id, audit::ActorType::User)
                        .org(org_id)
                        .target("frontline_device", id.to_string(), String::new()),
                )
                .await;
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(DeviceError::NotFound) => json_error(StatusCode::NOT_FOUND, "no such device"),
        Err(e) => {
            warn!(error = %e, "kiosk device revoke failed");
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "revoke failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
        h
    }

    #[test]
    fn the_kiosk_cookie_is_read_beside_the_session_cookie() {
        assert_eq!(
            extract_kiosk_cookie(&headers("oxy_session=jwt; oxy_kiosk=abc.def; x=y")).as_deref(),
            Some("abc.def")
        );
        assert!(extract_kiosk_cookie(&headers("oxy_session=jwt")).is_none());
        // An empty value is no value — the guard callers drifted on before.
        assert!(extract_kiosk_cookie(&headers("oxy_kiosk=; oxy_session=jwt")).is_none());
    }

    #[test]
    fn digest_comparison_rejects_length_and_content_mismatches() {
        let a = sha256_hex("secret");
        assert!(digest_eq(&a, &sha256_hex("secret")));
        assert!(!digest_eq(&a, &sha256_hex("secret2")));
        assert!(!digest_eq(&a, &a[..10]));
    }

    #[test]
    fn a_cookie_gated_response_is_marked_uncacheable() {
        let mut r = Json(serde_json::json!({ "bound": false })).into_response();
        no_store(&mut r);
        assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store, private");
        assert_eq!(r.headers()[header::VARY], "Cookie");
    }

    #[test]
    fn the_confirm_page_escapes_what_it_prints() {
        assert_eq!(
            escape("Front <counter> & \"bar\""),
            "Front &lt;counter&gt; &amp; &quot;bar&quot;"
        );
    }

    #[test]
    fn a_secret_is_long_and_never_repeats() {
        let a = random_secret();
        assert_eq!(a.len(), 64);
        assert_ne!(a, random_secret());
    }

    #[test]
    fn the_cookie_carries_the_attributes_the_session_cookie_does() {
        let c = kiosk_cookie("id.secret", true);
        for part in [
            "oxy_kiosk=id.secret",
            "Path=/",
            "HttpOnly",
            "SameSite=Lax",
            "Secure",
        ] {
            assert!(c.contains(part), "{c} lacks {part}");
        }
        assert!(!kiosk_cookie("id.secret", false).contains("Secure"));
    }
}
