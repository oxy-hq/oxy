//! The device-code flow's store (`oxyc … --no-browser`; RFC 8628 in shape).
//!
//! The loopback flow ([`super::cli_login`]) breaks wherever the approving
//! browser cannot reach the CLI's `127.0.0.1` listener: a cloud container, a
//! CI runner, a remote dev box. Here nothing travels back through the browser.
//!
//! 1. The CLI, holding no credential, starts a request ([`start`]) with its
//!    S256 challenge, its hostname and its *ask* — the `/cli-auth` parameters
//!    the loopback flow would have put in the URL. It is handed a secret
//!    **device code** and a short **user code**.
//! 2. A person opens the page on any device, signed in, and finds the request
//!    by its user code ([`pending`]). Approving it ([`approve`]) records the
//!    approver and the mint their session was checked for; refusing it
//!    ([`deny`]) records why.
//! 3. The CLI polls ([`poll`]) with the device code **and the PKCE verifier**,
//!    and collects the approval once. The caller mints from it exactly as the
//!    loopback exchange mints from a redeemed code.
//!
//! ## What each secret proves
//!
//! - The **device code** names the request to the poller; only its SHA-256 is
//!   stored, so a database reader cannot poll.
//! - The **verifier** proves the poller is the CLI that started the request.
//!   A poll with the right device code and a wrong verifier burns the row: a
//!   leaked device code alone collects nothing, and gets no second guess.
//! - The **user code** only finds the request for a signed-in person. It
//!   collects nothing, and the page shows it so the person can compare it with
//!   the one their terminal printed.
//!
//! ## A request may name who approves it
//!
//! `oxyc --approver <email>` names the person to ask ([`resolve_approver`]).
//! When it resolves to an active user, the request is bound to them: the page's
//! read, approve and deny answer as for an unknown code to anyone else
//! ([`Pending::may_decide`]). The binding is never a second factor — the user
//! code still has to be typed — it only keeps a request meant for one person
//! from being decided by another who got hold of the code.
//!
//! ## A request asks for one kind, and is approved as that kind
//!
//! A request whose ask names a `kind` is approved only with a mint, and one
//! that names none only as a login ([`approve`] refuses the mismatch), so an
//! approval can never turn an agent's narrow request into an all-access login.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Duration, Utc};
use entity::cli_device_requests::{self, Column};
use entity::prelude::CliDeviceRequests;
use oxy_shared::errors::OxyError;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, Set,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::cli_login::{Redeemed, VERIFIER_LEN, challenge_of};
pub use super::device_approver::{clean_approver, resolve as resolve_approver};
#[cfg(test)]
use super::device_ask::USER_CODE_ALPHABET;
use super::device_ask::new_user_code;
pub use super::device_ask::{
    AskedKind, USER_CODE_LEN, asked_kind, carries_user_code, clean_ask, clean_user_code,
    display_user_code,
};

/// How long a request waits for a decision.
pub const REQUEST_TTL_SECS: i64 = 10 * 60;
/// How often the CLI may poll; sooner answers [`Polled::SlowDown`].
pub const POLL_INTERVAL_SECS: i64 = 5;
/// How long an approval made near the end of the window stays collectable.
const COLLECT_GRACE_SECS: i64 = 60;
/// Lapsed requests are swept this long after they expire.
const SWEEP_AFTER_SECS: i64 = 60 * 60;
/// A fresh user code collides with a live one this rarely; a few tries is
/// plenty, and running out is an error rather than a loop.
const USER_CODE_TRIES: usize = 4;

pub const PENDING: &str = "pending";
pub const APPROVED: &str = "approved";
pub const DENIED: &str = "denied";

fn db_err(what: &'static str) -> impl FnOnce(sea_orm::DbErr) -> OxyError {
    move |e| OxyError::DBError(format!("{what}: {e}"))
}

fn hash_device_code(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

/// What [`start`] hands the CLI. The device code is shown once, here.
#[derive(Clone, Debug)]
pub struct Started {
    pub device_code: String,
    /// Stored form, without the dash: see [`display_user_code`].
    pub user_code: String,
    pub expires_in: i64,
    pub interval: i64,
    /// When the row was written: what a count of the requests before it is
    /// taken as of.
    pub created_at: DateTime<Utc>,
}

/// The approver a request is bound to when it named an email nobody holds
/// (or one two accounts share up to case): the nil id matches no user, so
/// nobody can decide it, and it lapses — exactly as a request bound to a real
/// person reads to everyone else. Without it, whether a named address belongs
/// to a user would show in who may open the request. A typo in `--approver`
/// therefore means rerunning with the right address.
pub const UNRESOLVED_APPROVER: Uuid = Uuid::nil();

/// How many pending requests bound to `user_id` were live as of `as_of`
/// (those created then or before): what an anonymous caller naming that
/// person can have waiting, and so how many notices they can aim at them
/// (the handler tells the approver only under the cap). Counted as of the
/// request's own write, so a request that lands later cannot withhold an
/// earlier one's notice, however the two tasks are scheduled.
pub async fn bound_pending<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    as_of: DateTime<Utc>,
) -> Result<u64, OxyError> {
    CliDeviceRequests::find()
        .filter(Column::ApproverUserId.eq(user_id))
        .filter(Column::Status.eq(PENDING))
        .filter(Column::CreatedAt.lte(as_of.fixed_offset()))
        .filter(Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        .count(db)
        .await
        .map_err(db_err("count cli device requests"))
}

/// Open a request. The caller has validated the challenge, the hostname and
/// the ask ([`clean_ask`]); `approver` is who may decide it, when it names one.
pub async fn start<C: ConnectionTrait>(
    db: &C,
    code_challenge: &str,
    hostname: &str,
    ask: Value,
    approver: Option<Uuid>,
) -> Result<Started, OxyError> {
    let now = Utc::now();
    sweep(db, now).await;
    let device_code = URL_SAFE_NO_PAD.encode(rand::random::<[u8; 32]>());
    for _ in 0..USER_CODE_TRIES {
        let user_code = new_user_code();
        let row = cli_device_requests::ActiveModel {
            device_code_hash: Set(hash_device_code(&device_code)),
            user_code: Set(user_code.clone()),
            code_challenge: Set(code_challenge.to_string()),
            hostname: Set(hostname.to_string()),
            ask: Set(ask.clone()),
            status: Set(PENDING.to_string()),
            user_id: Set(None),
            mint: Set(None),
            denial: Set(None),
            created_at: Set(now.fixed_offset()),
            expires_at: Set((now + Duration::seconds(REQUEST_TTL_SECS)).fixed_offset()),
            decided_at: Set(None),
            last_polled_at: Set(None),
            consumed_at: Set(None),
            approver_user_id: Set(approver),
        };
        match row.insert(db).await {
            Ok(_) => {
                return Ok(Started {
                    device_code,
                    user_code,
                    expires_in: REQUEST_TTL_SECS,
                    interval: POLL_INTERVAL_SECS,
                    created_at: now,
                });
            }
            // The unique index on `user_code`: draw another.
            Err(e) if is_unique_violation(&e) => continue,
            Err(e) => return Err(db_err("store cli device request")(e)),
        }
    }
    Err(OxyError::DBError(
        "could not draw a free user code for a cli device request".into(),
    ))
}

fn is_unique_violation(e: &sea_orm::DbErr) -> bool {
    matches!(
        e.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

/// Best-effort: the table holds only requests a CLI may still be waiting on.
async fn sweep<C: ConnectionTrait>(db: &C, now: DateTime<Utc>) {
    let cutoff = (now - Duration::seconds(SWEEP_AFTER_SECS)).fixed_offset();
    if let Err(e) = CliDeviceRequests::delete_many()
        .filter(Column::ExpiresAt.lt(cutoff))
        .exec(db)
        .await
    {
        tracing::warn!(error = %e, "could not sweep lapsed cli device requests");
    }
}

/// A request waiting on a person, as the page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub user_code: String,
    pub hostname: String,
    pub code_challenge: String,
    pub ask: Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// The one person who may decide it; `None` when the CLI named nobody.
    pub approver_user_id: Option<Uuid>,
}

impl Pending {
    /// Whether `user_id` may read and decide this request: anyone, unless it
    /// is bound to an approver, and then only them.
    pub fn may_decide(&self, user_id: Uuid) -> bool {
        self.approver_user_id
            .is_none_or(|approver| approver == user_id)
    }
}

/// The live, undecided request a user code names, or `None`.
pub async fn pending<C: ConnectionTrait>(
    db: &C,
    user_code: &str,
) -> Result<Option<Pending>, OxyError> {
    let row = CliDeviceRequests::find()
        .filter(Column::UserCode.eq(user_code))
        .filter(Column::Status.eq(PENDING))
        .filter(Column::ExpiresAt.gt(Utc::now().fixed_offset()))
        .one(db)
        .await
        .map_err(db_err("read cli device request"))?;
    Ok(row.map(|row| Pending {
        user_code: row.user_code,
        hostname: row.hostname,
        code_challenge: row.code_challenge,
        ask: row.ask,
        created_at: row.created_at.into(),
        expires_at: row.expires_at.into(),
        approver_user_id: row.approver_user_id,
    }))
}

/// Record a decision on the live, pending request `user_code` names. `false`
/// when there is none — it lapsed, or was decided first by another tab.
async fn decide<C: ConnectionTrait>(
    db: &C,
    user_code: &str,
    user_id: Uuid,
    outcome: (&'static str, Option<Value>, Option<String>),
) -> Result<bool, OxyError> {
    let (status, mint, denial) = outcome;
    let now = Utc::now().fixed_offset();
    let decided = CliDeviceRequests::update_many()
        .col_expr(Column::Status, Expr::value(status))
        .col_expr(Column::UserId, Expr::value(Some(user_id)))
        .col_expr(Column::Mint, Expr::value(mint))
        .col_expr(Column::Denial, Expr::value(denial))
        .col_expr(Column::DecidedAt, Expr::value(Some(now)))
        .filter(Column::UserCode.eq(user_code))
        .filter(Column::Status.eq(PENDING))
        .filter(Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(db_err("decide cli device request"))?;
    Ok(decided.rows_affected == 1)
}

/// Approve the request as `user_id`, with what their session was checked to
/// mint (`None` for a login). Refused when the mint is not of the kind the
/// request asked for. `Ok(false)` when there is no pending request to approve.
pub async fn approve<C: ConnectionTrait>(
    db: &C,
    pending: &Pending,
    user_id: Uuid,
    mint: Option<Value>,
) -> Result<bool, OxyError> {
    let matches = match asked_kind(&pending.ask) {
        AskedKind::Login => mint.is_none(),
        AskedKind::Mint => mint.is_some(),
    };
    if !matches {
        return Err(OxyError::ArgumentError(
            "an approval must be of the kind of token the request asked for".into(),
        ));
    }
    decide(db, &pending.user_code, user_id, (APPROVED, mint, None)).await
}

/// Refuse the request as `user_id`, saying why. `Ok(false)` when there is no
/// pending request to refuse.
pub async fn deny<C: ConnectionTrait>(
    db: &C,
    user_code: &str,
    user_id: Uuid,
    reason: &str,
) -> Result<bool, OxyError> {
    decide(
        db,
        user_code,
        user_id,
        (DENIED, None, Some(reason.to_string())),
    )
    .await
}

/// What one poll found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Polled {
    /// No decision yet: poll again after the interval.
    Pending,
    /// Polled sooner than the interval allows.
    SlowDown,
    /// Refused, with the reason the page was given.
    Denied(String),
    /// No decision came in time.
    Expired,
    /// No such request, already collected, or a verifier that does not match
    /// (which burns it). One answer for all of them.
    Invalid,
    /// Approved, and collected by this poll: mint from it, once.
    Approved(Redeemed),
}

/// Poll the request `device_code` names, proving with `code_verifier` that
/// the poller started it.
pub async fn poll<C: ConnectionTrait>(
    db: &C,
    device_code: &str,
    code_verifier: &str,
) -> Result<Polled, OxyError> {
    let now = Utc::now();
    let hash = hash_device_code(device_code.trim());
    let Some(row) = CliDeviceRequests::find_by_id(hash.clone())
        .one(db)
        .await
        .map_err(db_err("read cli device request"))?
    else {
        return Ok(Polled::Invalid);
    };
    if row.consumed_at.is_some() {
        return Ok(Polled::Invalid);
    }
    let verifier_ok = VERIFIER_LEN.contains(&code_verifier.len())
        && challenge_of(code_verifier) == row.code_challenge;
    if !verifier_ok {
        consume(db, hash, now).await?;
        return Ok(Polled::Invalid);
    }
    if !charge_poll(db, &hash, now).await? {
        return Ok(Polled::SlowDown);
    }
    // A refusal is reported whenever it is polled: its reason outlives the window.
    if row.status == DENIED {
        return Ok(Polled::Denied(row.denial.unwrap_or_default()));
    }
    if live_until(&row) <= now {
        return Ok(Polled::Expired);
    }
    if row.status == APPROVED {
        return collect(db, hash, row, now).await;
    }
    Ok(Polled::Pending)
}

/// Until when a poll may still find something: the request's window, or —
/// for an approval made near its end — a short grace to collect it.
fn live_until(row: &cli_device_requests::Model) -> DateTime<Utc> {
    let expires = DateTime::<Utc>::from(row.expires_at);
    match (row.status.as_str(), row.decided_at) {
        (APPROVED, Some(at)) => {
            expires.max(DateTime::<Utc>::from(at) + Duration::seconds(COLLECT_GRACE_SECS))
        }
        _ => expires,
    }
}

/// Record this poll, unless the last one was under an interval ago. One
/// conditional `UPDATE`, so two pollers racing cannot both slip under it. A
/// second's slack absorbs a CLI timer that fires a hair early.
async fn charge_poll<C: ConnectionTrait>(
    db: &C,
    hash: &[u8],
    now: DateTime<Utc>,
) -> Result<bool, OxyError> {
    let earliest = (now - Duration::seconds(POLL_INTERVAL_SECS - 1)).fixed_offset();
    let charged = CliDeviceRequests::update_many()
        .col_expr(Column::LastPolledAt, Expr::value(Some(now.fixed_offset())))
        .filter(Column::DeviceCodeHash.eq(hash.to_vec()))
        .filter(
            Condition::any()
                .add(Column::LastPolledAt.is_null())
                .add(Column::LastPolledAt.lte(earliest)),
        )
        .exec(db)
        .await
        .map_err(db_err("record cli device poll"))?;
    Ok(charged.rows_affected == 1)
}

/// Mark the row spent. `false` when another poll spent it first.
async fn consume<C: ConnectionTrait>(
    db: &C,
    hash: Vec<u8>,
    now: DateTime<Utc>,
) -> Result<bool, OxyError> {
    let spent = CliDeviceRequests::update_many()
        .col_expr(Column::ConsumedAt, Expr::value(Some(now.fixed_offset())))
        .filter(Column::DeviceCodeHash.eq(hash))
        .filter(Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(db_err("spend cli device request"))?;
    Ok(spent.rows_affected == 1)
}

/// Collect an approval, once: whichever poll spends the row first mints.
async fn collect<C: ConnectionTrait>(
    db: &C,
    hash: Vec<u8>,
    row: cli_device_requests::Model,
    now: DateTime<Utc>,
) -> Result<Polled, OxyError> {
    let Some(user_id) = row.user_id else {
        return Ok(Polled::Invalid);
    };
    // The kind was checked at the approval; checked again here, so a row
    // edited by hand still cannot collect a login for an agent's request.
    if (asked_kind(&row.ask) == AskedKind::Mint) != row.mint.is_some() {
        return Ok(Polled::Invalid);
    }
    if !consume(db, hash, now).await? {
        return Ok(Polled::Invalid);
    }
    Ok(Polled::Approved(Redeemed {
        user_id,
        hostname: row.hostname,
        mint: row.mint,
    }))
}

#[cfg(test)]
#[path = "device_login_tests.rs"]
mod tests;
