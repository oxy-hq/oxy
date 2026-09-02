//! DB-backed tests for device-token registration.
//!
//! These exist because the semantics under review are all *concurrency and
//! constraint* semantics, and the previous shape — `delete_many` then `insert`
//! — passed every test that only called it once. The bugs only appear against a
//! real `UNIQUE (platform, token)` index: a second registration raced into a
//! constraint violation, and a failed insert left the device deregistered
//! because its delete had already committed on its own.
//!
//! Run with:
//! `cargo nextest run -p oxy-app --test platform -E 'test(notification_devices)'`

use chrono::{Duration, Utc};
use entity::{device_tokens, users};
use sea_orm::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};
use uuid::Uuid;

use oxy_app::server::api::notifications::handlers::{prune_devices, upsert_device};

use crate::common::{Schema, fresh_db};

/// `device_tokens.user_id` carries a real FK, so a bare uuid is rejected —
/// which is the point of running these against a live schema at all.
async fn seed_user(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::new_v4();
    users::ActiveModel {
        id: ActiveValue::Set(id),
        email: ActiveValue::Set(Some(format!("device-{id}@example.com"))),
        name: ActiveValue::Set("Device Test".into()),
        picture: ActiveValue::Set(None),
        email_verified: ActiveValue::Set(true),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("seed user");
    id
}

async fn rows_for(db: &DatabaseConnection, user: Uuid) -> Vec<device_tokens::Model> {
    device_tokens::Entity::find()
        .filter(device_tokens::Column::UserId.eq(user))
        .order_by_desc(device_tokens::Column::LastSeenAt)
        .all(db)
        .await
        .expect("query device tokens")
}

/// Re-registering the same device is the normal case — a client retry, an app
/// foregrounded twice — and must be idempotent rather than a constraint
/// violation. The delete-then-insert shape returned 500 here.
#[tokio::test]
async fn re_registering_the_same_token_is_idempotent() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let user = seed_user(&db).await;
    let t0 = Utc::now().fixed_offset();

    upsert_device(&db, user, "web", "tok-a", Some("Pixel"), t0)
        .await
        .expect("first registration");
    let later = t0 + Duration::minutes(5);
    upsert_device(&db, user, "web", "tok-a", Some("Pixel"), later)
        .await
        .expect("second registration must not violate the unique index");

    let rows = rows_for(&db, user).await;
    assert_eq!(rows.len(), 1, "a re-register wrote a second row");
    // Compared at MICROSECOND resolution, which is what `timestamptz` stores.
    // Asserting on the `DateTime` directly compares nanoseconds the database
    // truncated on the way in, so the test only passes where the platform clock
    // happens to be microsecond-aligned — green on macOS, red on Linux CI.
    let micros = |t: chrono::DateTime<chrono::FixedOffset>| t.timestamp_micros();
    // The column exists to prune devices that stopped refreshing, so it has to
    // actually move — before the upsert it was only ever a copy of created_at.
    assert_eq!(
        micros(rows[0].last_seen_at),
        micros(later),
        "last_seen_at did not advance"
    );
    assert_eq!(
        micros(rows[0].created_at),
        micros(t0),
        "created_at should not move"
    );
    assert!(
        rows[0].last_seen_at > rows[0].created_at,
        "the two timestamps did not separate"
    );
}

/// A token identifies a device install, not a person: a phone passed on, or an
/// app reinstalled under a different login, hands the same token to a second
/// account. Exactly one row must survive, owned by whoever registered last —
/// otherwise two users share it and one receives the other's notifications.
#[tokio::test]
async fn a_token_moves_to_whoever_registered_last() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let first = seed_user(&db).await;
    let second = seed_user(&db).await;
    let t0 = Utc::now().fixed_offset();

    upsert_device(&db, first, "apns", "shared-token", None, t0)
        .await
        .expect("first owner");
    upsert_device(&db, second, "apns", "shared-token", None, t0)
        .await
        .expect("second owner");

    assert!(
        rows_for(&db, first).await.is_empty(),
        "the previous owner still holds the token and will receive the new owner's pushes"
    );
    let now_owned = rows_for(&db, second).await;
    assert_eq!(now_owned.len(), 1);
    assert_eq!(now_owned[0].token, "shared-token");
}

/// The same token on two platforms is two devices, not one — the unique index
/// is on the pair, and collapsing them would silently drop a registration.
#[tokio::test]
async fn platform_is_part_of_the_identity() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let user = seed_user(&db).await;
    let t0 = Utc::now().fixed_offset();

    upsert_device(&db, user, "web", "same", None, t0)
        .await
        .unwrap();
    upsert_device(&db, user, "fcm", "same", None, t0)
        .await
        .unwrap();

    assert_eq!(rows_for(&db, user).await.len(), 2);
}

/// `notify` loads every one of a user's rows on every notification, so the set
/// has to stay bounded. The prune drops the least recently seen.
#[tokio::test]
async fn pruning_keeps_the_most_recently_seen_and_bounds_the_set() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let user = seed_user(&db).await;
    let base = Utc::now().fixed_offset();

    // 25 devices, oldest first, so the newest are unambiguous.
    for i in 0..25i64 {
        upsert_device(
            &db,
            user,
            "web",
            &format!("tok-{i:02}"),
            None,
            base + Duration::minutes(i),
        )
        .await
        .unwrap();
    }
    assert_eq!(rows_for(&db, user).await.len(), 25, "fixture did not land");

    prune_devices(&db, user).await;

    let kept = rows_for(&db, user).await;
    assert_eq!(kept.len(), 20, "the set is still unbounded");
    let kept_tokens: Vec<&str> = kept.iter().map(|r| r.token.as_str()).collect();
    assert!(
        kept_tokens.contains(&"tok-24"),
        "the most recently seen device was pruned"
    );
    assert!(
        !kept_tokens.contains(&"tok-00"),
        "the least recently seen device survived"
    );

    // Idempotent: a second prune at the bound is a no-op, not a slow drain.
    prune_devices(&db, user).await;
    assert_eq!(rows_for(&db, user).await.len(), 20);
}

/// One user's registrations must never prune another's.
#[tokio::test]
async fn pruning_is_scoped_to_one_user() {
    let (db, _url) = fresh_db(Schema::Central).await;
    let noisy = seed_user(&db).await;
    let quiet = seed_user(&db).await;
    let base = Utc::now().fixed_offset();

    for i in 0..25i64 {
        upsert_device(
            &db,
            noisy,
            "web",
            &format!("noisy-{i:02}"),
            None,
            base + Duration::minutes(i),
        )
        .await
        .unwrap();
    }
    upsert_device(&db, quiet, "web", "quiet-1", None, base)
        .await
        .unwrap();

    prune_devices(&db, noisy).await;

    assert_eq!(rows_for(&db, quiet).await.len(), 1);
}
