//! Signing a browser in on a customer-owned domain.
//!
//! On the platform subdomain a visitor is signed in by the product's own
//! `oxy_session` cookie, which is scoped to the whole Oxy zone
//! (`Domain=.oxygen-hq.com`). A customer's `app.acme.com` never receives that
//! cookie, so it needs a cookie of its own, and the product host is the only
//! place a login happens. The handoff:
//!
//! 1. The custom domain sends an unauthenticated visitor to the product host's
//!    `authorize` route, with a random `state` it also sets as a cookie on
//!    itself.
//! 2. `authorize` (product host, the visitor's real login) checks the visitor
//!    may open the app, then [`open_ticket`]: it finds or mints the visitor's
//!    **domain token** and issues a five-minute, single-use ticket for it
//!    (`oxy_auth::token::browser_session`), and sends the browser back.
//! 3. The custom domain's `session` route checks `state` against its cookie,
//!    [`redeem`]s the ticket, and sets a **host-only** `oxy_session` carrying a
//!    token session (`browser_session::mint`).
//!
//! ## What the cookie on a customer's domain can do
//!
//! Whoever controls the domain's DNS can later point it elsewhere and read
//! every cookie a browser sends it. So the cookie is never the visitor's login.
//! It is a token session of a **personal token minted for that domain**:
//!
//! - one workspace grant — the workspace the app is published from — capped at
//!   `Member`, and never above the visitor's own role there;
//! - no Oxy-staff or partner standing, whatever the visitor holds;
//! - refused by every session-only route, as any token is;
//! - twelve hours at most ([`SESSION_HOURS`]), and revoked when the domain is
//!   removed ([`revoke_all`]) or the visitor logs out on it.
//!
//! The token is an ordinary row in `api_tokens` (source
//! [`source::CUSTOM_DOMAIN`]), so it shows in the visitor's token list, the
//! audit log and every revocation path. Nothing here mints authority the
//! visitor did not have.
//!
//! [`source::CUSTOM_DOMAIN`]: oxy_auth::token::credential::source::CUSTOM_DOMAIN

use chrono::{DateTime, Duration, Utc};
use entity::api_tokens;
use entity::prelude::{ApiTokens, Users};
use entity::users::{self, UserStatus};
use oxy_app_core::custom_domain_host::CustomDomainCtx;
use oxy_auth::token::browser_session::{self, Ticket};
use oxy_auth::token::credential::{StoredKind, source};
use oxy_auth::token::personal::{self, GrantSpec, NewToken};
use oxy_authz::RoleCeiling;
use oxy_shared::errors::OxyError;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use uuid::Uuid;

/// The longest a domain token, and so a session on the domain, lives.
pub const SESSION_HOURS: i64 = 12;

/// A token is reused for a new sign-in while it has at least this long left;
/// otherwise a fresh one is minted, so a session never opens minutes before
/// its token lapses.
const REUSE_MIN_REMAINING_MINS: i64 = 60;

/// What a domain token is named: the domain is the binding the redeem checks.
pub fn token_name(domain: &str) -> String {
    format!("custom-domain:{domain}")
}

/// A live domain token of `user_id` for `ctx`, minted when there is none with
/// enough life left, and a ticket that opens a session of it. The caller has
/// established that the user may open the app.
pub async fn open_ticket<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    ctx: &CustomDomainCtx,
) -> Result<Ticket, OxyError> {
    let now = Utc::now();
    let token_id = match reusable_token(db, user_id, ctx, now).await? {
        Some(id) => id,
        None => mint_token(db, user_id, ctx, now).await?,
    };
    browser_session::issue(db, user_id, token_id).await
}

/// A live domain token of `user_id` for `ctx` with enough life left — and
/// whose one grant still names the app's org and workspace. An app moved to
/// another workspace keeps its domain, so a token minted before the move would
/// otherwise be reused with a grant that no longer reaches the app.
async fn reusable_token<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    ctx: &CustomDomainCtx,
    now: DateTime<Utc>,
) -> Result<Option<Uuid>, OxyError> {
    let floor = now + Duration::minutes(REUSE_MIN_REMAINING_MINS);
    let name = token_name(&ctx.domain);
    let live: Vec<Uuid> = personal::live_named(db, user_id, &name, source::CUSTOM_DOMAIN)
        .await?
        .into_iter()
        .filter(|row| {
            row.expires_at
                .is_some_and(|at| DateTime::<Utc>::from(at) > floor)
        })
        .map(|row| row.id)
        .collect();
    let grants = personal::grants_for(db, &live).await?;
    Ok(live.into_iter().find(|id| {
        let mut own = grants.iter().filter(|g| g.token_id == *id);
        match (own.next(), own.next()) {
            (Some(grant), None) => {
                grant.org_id == ctx.org_id && grant.workspace_id == Some(ctx.project_id)
            }
            _ => false,
        }
    }))
}

async fn mint_token<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    ctx: &CustomDomainCtx,
    now: DateTime<Utc>,
) -> Result<Uuid, OxyError> {
    let minted = personal::create(
        db,
        NewToken {
            user_id,
            name: token_name(&ctx.domain),
            all_access: false,
            platform: false,
            partner: false,
            grants: vec![GrantSpec {
                org_id: ctx.org_id,
                workspace_id: Some(ctx.project_id),
                ceiling: RoleCeiling::Member,
            }],
            expires_at: Some(now + Duration::hours(SESSION_HOURS)),
            source: source::CUSTOM_DOMAIN,
        },
    )
    .await?;
    Ok(minted.row.id)
}

/// Why a ticket opened no session on a domain. Every case answers the visitor
/// the same way; the variant is for the log.
#[derive(Debug, thiserror::Error)]
pub enum RedeemError {
    #[error("the ticket is unknown, spent or expired")]
    InvalidTicket,
    #[error("the ticket was issued for another domain or another kind of token")]
    WrongBinding,
    #[error("the ticket's user is no longer active")]
    InactiveUser,
    #[error(transparent)]
    Internal(#[from] OxyError),
}

/// Spend `ticket` on `domain`: the domain token it opens and that token's user.
/// Refuses a ticket of any other token — an `oxyc login-link` ticket, or one
/// issued for a different domain — so a ticket cannot be carried across hosts.
pub async fn redeem<C: ConnectionTrait>(
    db: &C,
    ticket: &str,
    domain: &str,
) -> Result<(api_tokens::Model, users::Model), RedeemError> {
    let token_id = browser_session::redeem(db, ticket)
        .await?
        .ok_or(RedeemError::InvalidTicket)?;
    let row = ApiTokens::find_by_id(token_id)
        .one(db)
        .await
        .map_err(|e| OxyError::DBError(format!("load domain token: {e}")))?
        .ok_or(RedeemError::InvalidTicket)?;
    if !is_domain_token_for(&row, domain, Utc::now()) {
        return Err(RedeemError::WrongBinding);
    }
    let user = Users::find_by_id(row.principal_user_id)
        .one(db)
        .await
        .map_err(|e| OxyError::DBError(format!("load domain token user: {e}")))?
        .filter(|u| u.status == UserStatus::Active)
        .ok_or(RedeemError::InactiveUser)?;
    Ok((row, user))
}

/// A live personal token minted for `domain` by this module.
pub fn is_domain_token_for(row: &api_tokens::Model, domain: &str, now: DateTime<Utc>) -> bool {
    row.kind == StoredKind::Personal.as_str()
        && row.source == source::CUSTOM_DOMAIN
        && row.name == token_name(domain)
        && row.revoked_at.is_none()
        && row
            .expires_at
            .is_some_and(|at| DateTime::<Utc>::from(at) > now)
}

/// Logout on a custom domain: revoke the domain token the session `jwt` was
/// opened from, so the cookie stops working even where the browser kept a
/// copy. Only a JWT that verifies against its own token row, and only a domain
/// token of `domain`: a forged `kid` revokes nothing. Best effort — a failure
/// is logged, and the cookie is cleared regardless.
pub async fn end_session<C: ConnectionTrait>(db: &C, jwt: &str, domain: &str) {
    let Some(token_id) = browser_session::token_id_of(jwt) else {
        return;
    };
    let row = match ApiTokens::find_by_id(token_id).one(db).await {
        Ok(Some(row)) => row,
        Ok(None) => return,
        Err(e) => {
            tracing::warn!("custom domain logout: token lookup failed: {e}");
            return;
        }
    };
    if !is_domain_token_for(&row, domain, Utc::now()) || browser_session::verify(jwt, &row).is_err()
    {
        return;
    }
    let by = row.principal_user_id;
    match personal::revoke(db, row, by, "signed out on the custom domain").await {
        Ok(_) => oxy_auth::token::cache::invalidate_token(token_id),
        Err(e) => tracing::warn!("custom domain logout: revoke failed: {e}"),
    }
}

/// Why a domain's sessions were ended, as the visitor's token list shows it.
pub mod revoke_reason {
    pub const DOMAIN_REMOVED: &str = "custom domain removed";
    pub const APP_DELETED: &str = "app deleted";
    pub const APP_MOVED: &str = "app moved to another workspace";
}

/// What a revoke changed: the tokens it revoked and the domains whose routing
/// answer changed. The caches forget them only once the revoke has committed
/// ([`Revoked::forget`]); forgetting inside the transaction would let a request
/// in between re-cache a token that is still live in every other connection's
/// view, for the token cache's whole TTL.
#[must_use = "call forget() once the revoke has committed"]
#[derive(Debug, Default)]
pub struct Revoked {
    tokens: Vec<Uuid>,
    domains: Vec<String>,
}

impl Revoked {
    /// Drop the revoked tokens and the domains from this process's caches.
    /// Call after the transaction that revoked them commits.
    pub fn forget(self) {
        for id in self.tokens {
            oxy_auth::token::cache::invalidate_token(id);
        }
        for domain in &self.domains {
            oxy_app_core::custom_domain_host::invalidate(domain);
        }
    }

    /// How many tokens were revoked.
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    fn absorb(&mut self, other: Revoked) {
        self.tokens.extend(other.tokens);
        self.domains.extend(other.domains);
    }
}

/// Revoke every live domain token of `domain`, by anyone, recording `reason`
/// (one of [`revoke_reason`]). The caller forgets the result after commit.
pub async fn revoke_all<C: ConnectionTrait>(
    db: &C,
    domain: &str,
    by: Option<Uuid>,
    reason: &str,
) -> Result<Revoked, OxyError> {
    let live = ApiTokens::find()
        .filter(api_tokens::Column::Source.eq(source::CUSTOM_DOMAIN))
        .filter(api_tokens::Column::Name.eq(token_name(domain)))
        .filter(api_tokens::Column::RevokedAt.is_null())
        .all(db)
        .await
        .map_err(|e| OxyError::DBError(format!("find domain tokens: {e}")))?;
    let domains = vec![domain.to_string()];
    if live.is_empty() {
        return Ok(Revoked {
            tokens: Vec::new(),
            domains,
        });
    }
    let ids: Vec<Uuid> = live.iter().map(|row| row.id).collect();
    ApiTokens::update_many()
        .col_expr(
            api_tokens::Column::RevokedAt,
            Expr::value(Utc::now().fixed_offset()),
        )
        .col_expr(api_tokens::Column::RevokedBy, Expr::value(by))
        .col_expr(api_tokens::Column::RevokeReason, Expr::value(reason))
        .filter(api_tokens::Column::Id.is_in(ids.clone()))
        .exec(db)
        .await
        .map_err(|e| OxyError::DBError(format!("revoke domain tokens: {e}")))?;
    Ok(Revoked {
        tokens: ids,
        domains,
    })
}

/// Revoke the domain tokens of every domain `app_id` has verified — before the
/// app row is deleted (which cascades the domain rows away), or when the app
/// moves to another workspace (the tokens' grant names the old one).
pub async fn revoke_for_app<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    by: Option<Uuid>,
    reason: &str,
) -> Result<Revoked, OxyError> {
    let domains = super::list(db, app_id)
        .await
        .map_err(|e| OxyError::DBError(e.to_string()))?;
    // A pending claim never routed, so no session was ever opened on it: the
    // hostname's tokens, if any, belong to whichever app verified it.
    let mut revoked = Revoked::default();
    for row in domains.into_iter().filter(|row| row.verified_at.is_some()) {
        revoked.absorb(revoke_all(db, &row.domain, by, reason).await?);
    }
    Ok(revoked)
}
