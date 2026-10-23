//! Customer-owned domains for a custom app: claim one, prove it, remove it,
//! and the sign-in handoff that gives a browser a session on it.
//!
//! The read side every request touches — normalizing a hostname and resolving
//! a verified one to its app — is `oxy_app_core::custom_domain_host`. This
//! module is the write side and the session:
//!
//! - [`claim`] — an app asks for a hostname. A random token is stored; the
//!   customer publishes it as `TXT _oxygen-challenge.<domain>`.
//! - [`verify`] — read that TXT record ([`dns`]) and, when it matches, mark the
//!   row verified. Only now does the domain route, take a certificate
//!   (`tls_ask`) or open a sign-in. One app may hold a verified hostname: the
//!   partial unique index arbitrates two orgs proving the same name.
//! - [`remove`] — delete the row and end every session opened on the domain.
//! - [`session`] — the sign-in handoff (read its docs for the threat model).
//!
//! A claim is not ownership, so several apps may claim one hostname while none
//! has proved it; that is what stops an org that types a domain first from
//! squatting a name it does not control.

pub mod dns;
pub mod session;

use chrono::Utc;
use entity::app_custom_domains;
use entity::prelude::AppCustomDomains;
use oxy_app_core::custom_domain_host::{
    self, DomainError, challenge_name, challenge_value, normalize_domain,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set, TransactionTrait,
};
use uuid::Uuid;

use dns::TxtResolver;

/// Why a domain write was refused.
#[derive(Debug, thiserror::Error)]
pub enum DomainOpError {
    #[error(transparent)]
    Invalid(#[from] DomainError),
    #[error("{0} is already added to this app")]
    Exists(String),
    #[error("{0} is already connected to another app")]
    Taken(String),
    #[error("No such domain on this app")]
    NotFound,
    #[error("database error: {0}")]
    Db(String),
}

impl From<sea_orm::DbErr> for DomainOpError {
    fn from(e: sea_orm::DbErr) -> Self {
        Self::Db(e.to_string())
    }
}

/// Every domain of `app_id`, oldest first.
pub async fn list<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
) -> Result<Vec<app_custom_domains::Model>, DomainOpError> {
    Ok(AppCustomDomains::find()
        .filter(app_custom_domains::Column::AppId.eq(app_id))
        .order_by_asc(app_custom_domains::Column::CreatedAt)
        .all(db)
        .await?)
}

/// Claim `input` for `app_id`: normalize it, refuse one this app already has
/// or another app has proved, and store a fresh challenge token.
pub async fn claim<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    input: &str,
    created_by: Uuid,
) -> Result<app_custom_domains::Model, DomainOpError> {
    let domain = normalize_domain(input)?;
    let existing = AppCustomDomains::find()
        .filter(app_custom_domains::Column::Domain.eq(&domain))
        .all(db)
        .await?;
    if existing.iter().any(|row| row.app_id == app_id) {
        return Err(DomainOpError::Exists(domain));
    }
    if existing.iter().any(|row| row.verified_at.is_some()) {
        return Err(DomainOpError::Taken(domain));
    }
    let row = app_custom_domains::ActiveModel {
        id: Set(Uuid::new_v4()),
        app_id: Set(app_id),
        domain: Set(domain),
        verification_token: Set(new_token()),
        verified_at: Set(None),
        last_checked_at: Set(None),
        last_check_error: Set(None),
        created_by: Set(Some(created_by)),
        created_at: Set(Utc::now().fixed_offset()),
    }
    .insert(db)
    .await?;
    Ok(row)
}

/// 32 lowercase hex characters (122 random bits): safe in any TXT record and
/// not guessable, though it is public once published.
fn new_token() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Check the row's TXT record and record the outcome. A verified row is
/// re-checked too, so the console's "Verify" also tells whether the proof is
/// still published; a verified row is not un-verified by a failed check —
/// removing a connected domain is an explicit act ([`remove`]).
pub async fn verify<C: ConnectionTrait>(
    db: &C,
    resolver: &dyn TxtResolver,
    app_id: Uuid,
    domain_id: Uuid,
) -> Result<app_custom_domains::Model, DomainOpError> {
    let row = find(db, app_id, domain_id).await?;
    let outcome = check_challenge(resolver, &row).await;
    let now = Utc::now().fixed_offset();
    let mut active: app_custom_domains::ActiveModel = row.clone().into();
    active.last_checked_at = Set(Some(now));
    active.last_check_error = Set(outcome.clone().err());
    if outcome.is_ok() && row.verified_at.is_none() {
        if proved_elsewhere(db, &row).await? {
            return Err(DomainOpError::Taken(row.domain));
        }
        active.verified_at = Set(Some(now));
    }
    let updated = active.update(db).await.map_err(|e| {
        // The partial unique index: another app proved it between our read
        // and this write.
        if e.to_string()
            .contains("idx_app_custom_domains_verified_domain")
        {
            DomainOpError::Taken(row.domain.clone())
        } else {
            DomainOpError::from(e)
        }
    })?;
    custom_domain_host::invalidate(&updated.domain);
    Ok(updated)
}

/// `Ok(())` when the challenge TXT record carries this row's token; otherwise
/// the sentence the console shows.
async fn check_challenge(
    resolver: &dyn TxtResolver,
    row: &app_custom_domains::Model,
) -> Result<(), String> {
    let name = challenge_name(&row.domain);
    let expected = challenge_value(&row.verification_token);
    let values = resolver.txt(&name).await.map_err(|e| e.to_string())?;
    if values.iter().any(|v| v.trim() == expected) {
        return Ok(());
    }
    Err(match values.is_empty() {
        true => format!("No TXT record found at {name}. DNS changes can take a few minutes."),
        false => format!(
            "The TXT record at {name} does not match. Expected {expected}, found {}.",
            values.join(", ")
        ),
    })
}

async fn proved_elsewhere<C: ConnectionTrait>(
    db: &C,
    row: &app_custom_domains::Model,
) -> Result<bool, DomainOpError> {
    Ok(AppCustomDomains::find()
        .filter(app_custom_domains::Column::Domain.eq(&row.domain))
        .filter(app_custom_domains::Column::Id.ne(row.id))
        .filter(app_custom_domains::Column::VerifiedAt.is_not_null())
        .one(db)
        .await?
        .is_some())
}

/// Delete the row and, when it was the verified one, revoke every session
/// token opened on the domain, so a cookie a browser still holds stops working
/// on every replica at once. One transaction, revoke first: a failure leaves
/// the row in place, and Remove can be pressed again. A pending claim's removal
/// revokes nothing — the hostname's sessions belong to the app that verified it.
pub async fn remove(
    db: &DatabaseConnection,
    app_id: Uuid,
    domain_id: Uuid,
    by: Uuid,
) -> Result<app_custom_domains::Model, DomainOpError> {
    let row = find(db, app_id, domain_id).await?;
    let txn = db.begin().await?;
    let revoked = if row.verified_at.is_some() {
        session::revoke_all(
            &txn,
            &row.domain,
            Some(by),
            session::revoke_reason::DOMAIN_REMOVED,
        )
        .await
        .map_err(|e| DomainOpError::Db(e.to_string()))?
    } else {
        session::Revoked::default()
    };
    AppCustomDomains::delete_by_id(row.id).exec(&txn).await?;
    txn.commit().await?;
    revoked.forget();
    custom_domain_host::invalidate(&row.domain);
    Ok(row)
}

/// The row `domain_id` names, when it belongs to `app_id`.
pub async fn find(
    db: &impl ConnectionTrait,
    app_id: Uuid,
    domain_id: Uuid,
) -> Result<app_custom_domains::Model, DomainOpError> {
    AppCustomDomains::find_by_id(domain_id)
        .filter(app_custom_domains::Column::AppId.eq(app_id))
        .one(db)
        .await?
        .ok_or(DomainOpError::NotFound)
}

/// `true` when `domain` is a verified custom domain — the TLS edge's question
/// before it asks a certificate authority for one. Reads through the resolver's
/// cache, so a probe storm costs one query per name per minute.
pub async fn is_verified(domain: &str) -> bool {
    let Some(candidate) = custom_domain_host::candidate_host(domain) else {
        return false;
    };
    custom_domain_host::resolve(&candidate).await.is_some()
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
