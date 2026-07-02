use entity::users::{self, UserStatus};

// Simple identity structure for email-based identity linking
#[derive(Debug, Clone)]
pub struct Identity {
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: uuid::Uuid,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
    pub status: UserStatus,
}

impl From<users::Model> for AuthenticatedUser {
    fn from(user: users::Model) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            status: user.status,
        }
    }
}

/// Request-extension marker: the request authenticated via an app publish token
/// (`oxypublish_...` bearer), not a session JWT/cookie or an API key.
///
/// App publish tokens are deliberately narrow — they authorize the customer-apps
/// admin surface only. This marker is what the scope-enforcement middleware
/// keys off to reject an app-publish-token request that targets any other route.
/// Its presence means "downstream must treat this identity as scope-limited."
#[derive(Debug, Clone, Copy)]
pub struct AppPublishTokenAuth {
    pub token_id: uuid::Uuid,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
