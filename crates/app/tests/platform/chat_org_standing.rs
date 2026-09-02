//! A `chat_channel_members` row is not, on its own, permission to read a channel.
//!
//! Its foreign keys reach `chat_channels` and `users` only, so nothing removes
//! it when somebody leaves the org. Both chat gates therefore have to re-check
//! org standing at read time, and this is the case that proves they do:
//! membership row present, org membership gone, access refused.
//!
//! Lives in `tests/platform/` rather than `tests/authz/` for a mechanical
//! reason, not a topical one: this uses `common::fresh_db`, which
//! `.config/nextest.toml` routes to `db-per-test` via `binary(=platform)` with
//! no list edit. The DB-touching cases in `tests/authz/` share one
//! `OXY_DATABASE_URL` and are pinned into `serial-db` by a registry that fails
//! the build when it drifts.
//!
//! Run with:
//! `cargo nextest run -p oxy-app --test platform -E 'test(chat_org_standing)'`

use chrono::Utc;
use entity::{
    chat_channel_members, chat_channels, org_frontline_members, org_members, organizations, users,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
};
use uuid::Uuid;

use oxy_app::server::api::chat::handlers::{member_channel, visible_channels};

use crate::common::{Schema, fresh_db};

struct Fixture {
    user: Uuid,
    org: Uuid,
    channel: Uuid,
}

/// One org, one member, one channel they belong to.
async fn seed(db: &DatabaseConnection) -> Fixture {
    let user = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(user),
        email: ActiveValue::Set(Some(format!("chat-{user}@example.com"))),
        name: ActiveValue::Set("Chat Test".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed user");

    let org = Uuid::new_v4();
    organizations::ActiveModel {
        id: ActiveValue::Set(org),
        name: ActiveValue::Set("Chat Org".into()),
        slug: ActiveValue::Set(format!("chat-org-{org}")),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed org");

    org_members::ActiveModel {
        id: ActiveValue::Set(Uuid::new_v4()),
        org_id: ActiveValue::Set(org),
        user_id: ActiveValue::Set(user),
        role: ActiveValue::Set(org_members::OrgRole::Member),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed org membership");

    let channel = Uuid::new_v4();
    chat_channels::ActiveModel {
        id: ActiveValue::Set(channel),
        org_id: ActiveValue::Set(org),
        kind: ActiveValue::Set("channel".into()),
        name: ActiveValue::Set(Some("ops".into())),
        topic: ActiveValue::Set(None),
        created_by: ActiveValue::Set(Some(user)),
        created_at: ActiveValue::Set(Utc::now().fixed_offset()),
        archived_at: ActiveValue::Set(None),
    }
    .insert(db)
    .await
    .expect("seed channel");

    chat_channel_members::ActiveModel {
        channel_id: ActiveValue::Set(channel),
        user_id: ActiveValue::Set(user),
        joined_at: ActiveValue::Set(Utc::now().fixed_offset()),
        last_read_at: ActiveValue::Set(None),
        muted: ActiveValue::Set(false),
    }
    .insert(db)
    .await
    .expect("seed channel membership");

    Fixture { user, org, channel }
}

async fn remove_from_org(db: &DatabaseConnection, user: Uuid, org: Uuid) {
    org_members::Entity::delete_many()
        .filter(org_members::Column::UserId.eq(user))
        .filter(org_members::Column::OrgId.eq(org))
        .exec(db)
        .await
        .expect("remove org membership");
}

async fn still_a_channel_member(db: &DatabaseConnection, f: &Fixture) -> bool {
    chat_channel_members::Entity::find_by_id((f.channel, f.user))
        .one(db)
        .await
        .expect("query channel membership")
        .is_some()
}

/// The gate on a single channel — reads, writes and the SSE stream all inherit
/// this one, so it is the load-bearing half.
#[tokio::test]
async fn a_removed_org_member_loses_the_channel() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let f = seed(&db).await;

    assert!(
        member_channel(&db, f.channel, f.user).await.is_some(),
        "fixture did not grant access in the first place"
    );

    remove_from_org(&db, f.user, f.org).await;

    assert!(
        still_a_channel_member(&db, &f).await,
        "the chat_channel_members row was deleted, so this test would pass for \
         the wrong reason — the whole point is that it survives"
    );
    assert!(
        member_channel(&db, f.channel, f.user).await.is_none(),
        "a removed org member still reaches the channel"
    );
}

/// The list gate. Separate from the one above because it was separately missing:
/// `member_channel` was fixed while `list_channels` went straight from
/// membership rows to summaries, so a removed member kept seeing the channel's
/// name, topic, member count and a live unread count.
#[tokio::test]
async fn a_removed_org_member_loses_the_channel_list() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let f = seed(&db).await;

    let before = visible_channels(&db, f.user).await.expect("list");
    assert_eq!(before.len(), 1, "fixture did not produce a visible channel");
    assert_eq!(before[0].1.id, f.channel);

    remove_from_org(&db, f.user, f.org).await;

    assert!(still_a_channel_member(&db, &f).await);
    assert!(
        visible_channels(&db, f.user)
            .await
            .expect("list")
            .is_empty(),
        "a removed org member still sees the channel in their list"
    );
}

/// A membership row for a channel in an org the user was never in must not grant
/// anything either — the gate is standing, not history.
#[tokio::test]
async fn a_membership_row_alone_grants_nothing() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let f = seed(&db).await;

    // A second user with a membership row but no org standing at all.
    let outsider = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(outsider),
        email: ActiveValue::Set(Some(format!("out-{outsider}@example.com"))),
        name: ActiveValue::Set("Outsider".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed outsider");
    chat_channel_members::ActiveModel {
        channel_id: ActiveValue::Set(f.channel),
        user_id: ActiveValue::Set(outsider),
        joined_at: ActiveValue::Set(Utc::now().fixed_offset()),
        last_read_at: ActiveValue::Set(None),
        muted: ActiveValue::Set(false),
    }
    .insert(&db)
    .await
    .expect("seed outsider membership");

    assert!(member_channel(&db, f.channel, outsider).await.is_none());
    assert!(
        visible_channels(&db, outsider)
            .await
            .expect("list")
            .is_empty()
    );
}

/// A frontline worker holds NO `org_members` row by design — they are enrolled
/// by PIN on a shared tablet — and they are the primary audience for these
/// channels.
///
/// This is the case the org-standing gate would have broken silently: checking
/// membership alone is correct for every office user and locks out exactly the
/// people chat exists for. It only became testable once
/// `org_frontline_members` landed on main.
#[tokio::test]
async fn an_active_frontline_worker_reaches_the_channel() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let f = seed(&db).await;

    // Same org, same channel, but standing comes from frontline enrolment only.
    let worker = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(worker),
        // No email: a frontline worker is a user who may not have a mailbox.
        email: ActiveValue::Set(None),
        name: ActiveValue::Set("Shift Lead".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed frontline user");
    org_frontline_members::ActiveModel {
        org_id: ActiveValue::Set(f.org),
        user_id: ActiveValue::Set(worker),
        status: ActiveValue::Set("active".into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("enrol frontline worker");
    chat_channel_members::ActiveModel {
        channel_id: ActiveValue::Set(f.channel),
        user_id: ActiveValue::Set(worker),
        joined_at: ActiveValue::Set(Utc::now().fixed_offset()),
        last_read_at: ActiveValue::Set(None),
        muted: ActiveValue::Set(false),
    }
    .insert(&db)
    .await
    .expect("add worker to channel");

    assert!(
        org_members::Entity::find()
            .filter(org_members::Column::UserId.eq(worker))
            .one(&db)
            .await
            .expect("query")
            .is_none(),
        "fixture gave the worker an org_members row, so it proves nothing"
    );
    assert!(
        member_channel(&db, f.channel, worker).await.is_some(),
        "a frontline worker cannot reach the channel they were added to"
    );
    assert_eq!(
        visible_channels(&db, worker).await.expect("list").len(),
        1,
        "a frontline worker sees an empty channel list"
    );
}

/// Suspending a worker takes their chat access with it — one row, one effect.
#[tokio::test]
async fn a_suspended_frontline_worker_loses_the_channel() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let f = seed(&db).await;

    let worker = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(worker),
        email: ActiveValue::Set(None),
        name: ActiveValue::Set("Suspended".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("seed user");
    org_frontline_members::ActiveModel {
        org_id: ActiveValue::Set(f.org),
        user_id: ActiveValue::Set(worker),
        status: ActiveValue::Set("suspended".into()),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("enrol suspended");
    chat_channel_members::ActiveModel {
        channel_id: ActiveValue::Set(f.channel),
        user_id: ActiveValue::Set(worker),
        joined_at: ActiveValue::Set(Utc::now().fixed_offset()),
        last_read_at: ActiveValue::Set(None),
        muted: ActiveValue::Set(false),
    }
    .insert(&db)
    .await
    .expect("add to channel");

    assert!(member_channel(&db, f.channel, worker).await.is_none());
    assert!(
        visible_channels(&db, worker)
            .await
            .expect("list")
            .is_empty()
    );
}
