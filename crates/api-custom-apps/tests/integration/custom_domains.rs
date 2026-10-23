//! Custom domains against real rows: a claim routes nothing until its TXT
//! record is proved, one app wins a contested name, a sign-in ticket opens a
//! session only on the domain it was issued for, and removing the domain ends
//! both the routing and every session opened on it.
//!
//! The DNS read is the one fake (a fixed `TxtResolver`); everything else — the
//! resolver cache, the partial unique index, the token and ticket tables — is
//! the code that runs in production.

use async_trait::async_trait;
use entity::prelude::{ApiTokenGrants, ApiTokens};
use entity::{api_token_grants, api_tokens};
use oxy_app_core::custom_domain_host::{self, challenge_value};
use oxy_custom_apps::custom_domains::dns::{LookupError, TxtResolver};
use oxy_custom_apps::custom_domains::session::{self, RedeemError};
use oxy_custom_apps::custom_domains::{self, DomainOpError};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use super::custom_app_visibility::{seed_app, seed_org, seed_user, seed_workspace};
use crate::common::test_db;

/// Publishes whatever TXT values it was built with, at every name.
struct Publishes(Vec<String>);

#[async_trait]
impl TxtResolver for Publishes {
    async fn txt(&self, _name: &str) -> Result<Vec<String>, LookupError> {
        Ok(self.0.clone())
    }
}

/// A fresh name per test: the resolver cache is process-wide.
fn domain() -> String {
    format!("app-{}.acme-test-domains.com", Uuid::new_v4().simple())
}

struct Fixture {
    db: DatabaseConnection,
    user_id: Uuid,
    workspace_id: Uuid,
    app_id: Uuid,
}

async fn fixture() -> Fixture {
    let db = test_db().await;
    let (user_id, _) = seed_user(&db).await;
    let org_id = seed_org(&db).await;
    let workspace_id = seed_workspace(&db, org_id).await;
    let app_id = seed_app(&db, org_id, workspace_id, "Store", false).await;
    Fixture {
        db,
        user_id,
        workspace_id,
        app_id,
    }
}

async fn claim_and_prove(f: &Fixture, name: &str) -> entity::app_custom_domains::Model {
    let row = custom_domains::claim(&f.db, f.app_id, name, f.user_id)
        .await
        .expect("claim");
    let proof = Publishes(vec![challenge_value(&row.verification_token)]);
    custom_domains::verify(&f.db, &proof, f.app_id, row.id)
        .await
        .expect("verify")
}

#[tokio::test]
async fn a_claim_routes_only_once_proved() {
    let f = fixture().await;
    let name = domain();
    let row = custom_domains::claim(&f.db, f.app_id, &name.to_uppercase(), f.user_id)
        .await
        .expect("claim");
    assert_eq!(row.domain, name, "stored normalized");
    assert!(custom_domain_host::resolve(&name).await.is_none());
    assert!(!custom_domains::is_verified(&name).await);

    // The wrong token is recorded as the reason, and proves nothing.
    let wrong = Publishes(vec!["oxygen-verification=nope".into()]);
    let checked = custom_domains::verify(&f.db, &wrong, f.app_id, row.id)
        .await
        .expect("verify runs");
    assert!(checked.verified_at.is_none());
    assert!(checked.last_check_error.unwrap().contains("does not match"));

    let proved = claim_and_prove(&f, &domain()).await;
    let ctx = custom_domain_host::resolve(&proved.domain)
        .await
        .expect("a verified domain routes");
    assert_eq!(ctx.app_id, f.app_id);
    assert_eq!(ctx.project_id, f.workspace_id);
    assert!(custom_domains::is_verified(&proved.domain).await);
}

#[tokio::test]
async fn one_app_wins_a_contested_name() {
    let first = fixture().await;
    let name = domain();
    // A second org claims the same name before anyone proves it: allowed, so
    // whoever typed it first cannot squat it.
    let (other_user, _) = seed_user(&first.db).await;
    let other_org = seed_org(&first.db).await;
    let other_ws = seed_workspace(&first.db, other_org).await;
    let other_app = seed_app(&first.db, other_org, other_ws, "Other", false).await;
    let theirs = custom_domains::claim(&first.db, other_app, &name, other_user)
        .await
        .expect("a pending claim does not block another");

    claim_and_prove(&first, &name).await;

    let proof = Publishes(vec![challenge_value(&theirs.verification_token)]);
    let refused = custom_domains::verify(&first.db, &proof, other_app, theirs.id).await;
    assert!(
        matches!(refused, Err(DomainOpError::Taken(_))),
        "{refused:?}"
    );

    let late = custom_domains::claim(&first.db, other_app, &name, other_user).await;
    assert!(matches!(late, Err(DomainOpError::Exists(_))), "{late:?}");
    let (third_user, _) = seed_user(&first.db).await;
    let third_app = seed_app(&first.db, other_org, other_ws, "Third", false).await;
    let after = custom_domains::claim(&first.db, third_app, &name, third_user).await;
    assert!(matches!(after, Err(DomainOpError::Taken(_))), "{after:?}");
}

#[tokio::test]
async fn the_session_token_is_narrow_and_bound_to_its_domain() {
    let f = fixture().await;
    let row = claim_and_prove(&f, &domain()).await;
    let ctx = custom_domain_host::resolve(&row.domain).await.unwrap();

    let ticket = session::open_ticket(&f.db, f.user_id, &ctx)
        .await
        .expect("ticket");
    let token = only_domain_token(&f.db, f.user_id).await;
    assert!(!token.all_access && !token.platform && !token.partner);
    let grants = ApiTokenGrants::find()
        .filter(api_token_grants::Column::TokenId.eq(token.id))
        .all(&f.db)
        .await
        .unwrap();
    assert_eq!(grants.len(), 1, "one workspace grant");

    // Carried to another host, the ticket opens nothing — and is spent.
    let elsewhere = session::redeem(&f.db, &ticket.ticket, "other.acme-test-domains.com").await;
    assert!(
        matches!(elsewhere, Err(RedeemError::WrongBinding)),
        "{elsewhere:?}"
    );
    let again = session::redeem(&f.db, &ticket.ticket, &ctx.domain).await;
    assert!(
        matches!(again, Err(RedeemError::InvalidTicket)),
        "{again:?}"
    );

    let fresh = session::open_ticket(&f.db, f.user_id, &ctx).await.unwrap();
    let (redeemed, user) = session::redeem(&f.db, &fresh.ticket, &ctx.domain)
        .await
        .expect("redeem on its own domain");
    assert_eq!(
        redeemed.id, token.id,
        "a live token is reused, not re-minted"
    );
    assert_eq!(user.id, f.user_id);
}

#[tokio::test]
async fn removing_a_domain_ends_routing_and_sessions() {
    let f = fixture().await;
    let row = claim_and_prove(&f, &domain()).await;
    let ctx = custom_domain_host::resolve(&row.domain).await.unwrap();
    session::open_ticket(&f.db, f.user_id, &ctx).await.unwrap();

    custom_domains::remove(&f.db, f.app_id, row.id, f.user_id)
        .await
        .expect("remove");
    assert!(custom_domain_host::resolve(&row.domain).await.is_none());
    let token = only_domain_token(&f.db, f.user_id).await;
    assert!(
        token.revoked_at.is_some(),
        "the domain's session token is revoked"
    );
    assert_eq!(
        token.revoke_reason.as_deref(),
        Some(session::revoke_reason::DOMAIN_REMOVED)
    );
}

#[tokio::test]
async fn a_move_ends_the_domains_sessions_and_says_so() {
    let f = fixture().await;
    let row = claim_and_prove(&f, &domain()).await;
    let ctx = custom_domain_host::resolve(&row.domain).await.unwrap();
    session::open_ticket(&f.db, f.user_id, &ctx).await.unwrap();

    let revoked = session::revoke_for_app(
        &f.db,
        f.app_id,
        Some(f.user_id),
        session::revoke_reason::APP_MOVED,
    )
    .await
    .expect("revoke");
    assert_eq!(revoked.token_count(), 1, "the revoke reports what it ended");
    revoked.forget();
    let token = only_domain_token(&f.db, f.user_id).await;
    assert!(token.revoked_at.is_some());
    assert_eq!(
        token.revoke_reason.as_deref(),
        Some(session::revoke_reason::APP_MOVED),
        "the domain is still attached, so the reason must not say it was removed"
    );
}

#[tokio::test]
async fn the_handoff_sees_a_move_this_replicas_cache_has_not() {
    use entity::apps;
    use sea_orm::{ActiveModelTrait, ActiveValue};

    let f = fixture().await;
    let row = claim_and_prove(&f, &domain()).await;
    let cached = custom_domain_host::resolve(&row.domain).await.unwrap();
    assert_eq!(cached.project_id, f.workspace_id);

    // Another replica moves the app: it invalidates its own cache, not ours.
    let new_ws = seed_workspace(&f.db, cached.org_id).await;
    apps::ActiveModel {
        id: ActiveValue::Unchanged(f.app_id),
        project_id: ActiveValue::Set(new_ws),
        ..Default::default()
    }
    .update(&f.db)
    .await
    .expect("move the app");
    let stale = custom_domain_host::resolve(&row.domain).await.unwrap();
    assert_eq!(
        stale.project_id, f.workspace_id,
        "precondition: stale cache"
    );

    let fresh = custom_domain_host::resolve_fresh(&row.domain)
        .await
        .unwrap();
    assert_eq!(fresh.project_id, new_ws);
    session::open_ticket(&f.db, f.user_id, &fresh)
        .await
        .unwrap();
    let token = only_domain_token(&f.db, f.user_id).await;
    let grants = ApiTokenGrants::find()
        .filter(api_token_grants::Column::TokenId.eq(token.id))
        .all(&f.db)
        .await
        .unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(
        grants[0].workspace_id,
        Some(new_ws),
        "the token is minted on the workspace the app now lives in"
    );
    assert_eq!(
        custom_domain_host::resolve(&row.domain)
            .await
            .unwrap()
            .project_id,
        new_ws,
        "the fresh read refreshes this replica's cache"
    );
}

#[tokio::test]
async fn removing_another_apps_pending_claim_leaves_the_sessions_alone() {
    let owner = fixture().await;
    let name = domain();
    // Another org claims the name first; it never proves it.
    let (squatter, _) = seed_user(&owner.db).await;
    let other_org = seed_org(&owner.db).await;
    let other_ws = seed_workspace(&owner.db, other_org).await;
    let other_app = seed_app(&owner.db, other_org, other_ws, "Squatter", false).await;
    let pending = custom_domains::claim(&owner.db, other_app, &name, squatter)
        .await
        .expect("pending claim");

    let row = claim_and_prove(&owner, &name).await;
    let ctx = custom_domain_host::resolve(&row.domain).await.unwrap();
    session::open_ticket(&owner.db, owner.user_id, &ctx)
        .await
        .unwrap();

    custom_domains::remove(&owner.db, other_app, pending.id, squatter)
        .await
        .expect("remove the pending claim");
    let token = only_domain_token(&owner.db, owner.user_id).await;
    assert!(
        token.revoked_at.is_none(),
        "the verified app's session token survives another app's removal"
    );
    assert!(custom_domain_host::resolve(&row.domain).await.is_some());
}

#[tokio::test]
async fn a_token_is_not_reused_after_the_app_moves_workspace() {
    use entity::apps;
    use sea_orm::{ActiveModelTrait, ActiveValue};

    let f = fixture().await;
    let row = claim_and_prove(&f, &domain()).await;
    let before = custom_domain_host::resolve(&row.domain).await.unwrap();
    let first = session::open_ticket(&f.db, f.user_id, &before)
        .await
        .unwrap();
    let (first_token, _) = session::redeem(&f.db, &first.ticket, &before.domain)
        .await
        .unwrap();

    let org_id = before.org_id;
    let new_ws = seed_workspace(&f.db, org_id).await;
    apps::ActiveModel {
        id: ActiveValue::Unchanged(f.app_id),
        project_id: ActiveValue::Set(new_ws),
        ..Default::default()
    }
    .update(&f.db)
    .await
    .expect("move the app");
    custom_domain_host::invalidate_all();
    let after = custom_domain_host::resolve(&row.domain).await.unwrap();
    assert_eq!(after.project_id, new_ws);

    let second = session::open_ticket(&f.db, f.user_id, &after)
        .await
        .unwrap();
    let (second_token, _) = session::redeem(&f.db, &second.ticket, &after.domain)
        .await
        .unwrap();
    assert_ne!(
        second_token.id, first_token.id,
        "a fresh token for the new workspace"
    );
    let grants = ApiTokenGrants::find()
        .filter(api_token_grants::Column::TokenId.eq(second_token.id))
        .all(&f.db)
        .await
        .unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].workspace_id, Some(new_ws));
}

async fn only_domain_token(db: &DatabaseConnection, user_id: Uuid) -> api_tokens::Model {
    let mut rows = ApiTokens::find()
        .filter(api_tokens::Column::PrincipalUserId.eq(user_id))
        .filter(api_tokens::Column::Source.eq("custom_domain"))
        .all(db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "exactly one domain token");
    rows.remove(0)
}
