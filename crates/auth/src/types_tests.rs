use super::*;
use entity::users::{self, UserStatus};
use uuid::Uuid;

#[test]
fn authenticated_user_from_model_preserves_fields() {
    let now = chrono::Utc::now().into();
    let model = users::Model {
        id: Uuid::new_v4(),
        email: Some("test@example.com".to_string()),
        name: "Test User".to_string(),
        picture: Some("https://example.com/pic.jpg".to_string()),
        email_verified: true,
        magic_link_token: None,
        magic_link_token_expires_at: None,
        status: UserStatus::Active,
        created_at: now,
        last_login_at: now,
    };

    let auth_user = AuthenticatedUser::from(model.clone());

    assert_eq!(auth_user.id, model.id);
    assert_eq!(auth_user.email.as_deref(), Some("test@example.com"));
    assert_eq!(auth_user.name, "Test User");
    assert_eq!(
        auth_user.picture,
        Some("https://example.com/pic.jpg".to_string())
    );
    assert_eq!(auth_user.status, UserStatus::Active);
}

#[test]
fn a_user_with_no_email_labels_as_their_name() {
    // Frontline identity: the address is optional, `name` is not. `label()` is
    // the display fallback, and the reason no `handle` column was needed.
    let now = chrono::Utc::now().into();
    let model = users::Model {
        id: Uuid::new_v4(),
        email: None,
        name: "Maria S.".to_string(),
        picture: None,
        email_verified: false,
        magic_link_token: None,
        magic_link_token_expires_at: None,
        status: UserStatus::Active,
        created_at: now,
        last_login_at: now,
    };
    assert_eq!(model.label(), "Maria S.");

    let auth_user = AuthenticatedUser::from(model);
    assert_eq!(
        auth_user.email, None,
        "no address must stay None, never \"\""
    );
    assert_eq!(auth_user.label(), "Maria S.");
}
