use sea_orm::{DatabaseBackend, QueryTrait};

use super::*;

fn user(id: Uuid) -> oxy_auth::types::AuthenticatedUser {
    oxy_auth::types::AuthenticatedUser {
        id,
        email: None,
        name: "Crew".into(),
        picture: None,
        status: entity::users::UserStatus::Active,
        credential: None,
    }
}

fn notification(user_id: Uuid) -> notifications::Model {
    notifications::Model {
        id: Uuid::new_v4(),
        org_id: Uuid::new_v4(),
        user_id,
        kind: "list_overdue".into(),
        title: String::new(),
        body: None,
        link: None,
        subject_kind: None,
        subject_id: None,
        created_at: Utc::now().fixed_offset(),
        read_at: None,
        message_id: None,
        app_id: None,
    }
}

/// `GET /api/notifications/{id}` and `/read` answer 404 for anything but the
/// caller's own row.
#[test]
fn a_notification_is_visible_only_to_its_recipient() {
    let me = Uuid::new_v4();
    assert!(visible_to(&notification(me), &user(me)));
    assert!(!visible_to(&notification(Uuid::new_v4()), &user(me)));
}

/// The DELETE is scoped to the caller: the statement names both the user and
/// the token, so another user's registration of the same token survives.
#[test]
fn unregistering_deletes_only_the_callers_row_for_the_token() {
    let me = Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("uuid");
    let sql = delete_device(me, "tok-a")
        .build(DatabaseBackend::Postgres)
        .to_string();
    assert_eq!(
        sql,
        r#"DELETE FROM "device_tokens" WHERE "device_tokens"."user_id" = '11111111-1111-1111-1111-111111111111' AND "device_tokens"."token" = 'tok-a'"#
    );
}
