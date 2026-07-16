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

impl AuthenticatedUser {
    /// A synthetic principal for an OIDC-minted, app-scoped machine publish token.
    /// It exists only to satisfy the extractor chain — the token-scope middleware
    /// confines it to the publish path, and the publish path authorizes by the
    /// token's `app_id` + client consent, never by this identity. The nil id makes
    /// it unmistakable in any log that it is not a real user.
    pub fn machine_publisher() -> Self {
        Self {
            id: uuid::Uuid::nil(),
            email: "oxy-publish-bot@oxy.internal".to_string(),
            name: "Oxy Publish (machine)".to_string(),
            picture: None,
            status: UserStatus::Active,
        }
    }
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
    /// Set for any **app-scoped** publish token — OIDC-minted (no human) or
    /// partner-minted (a real `created_by`, design §7). The publish path authorizes
    /// such a token strictly by this app id + the client's consent, so it is
    /// confined to that one app. `None` for an app-unscoped staff token.
    pub app_id: Option<uuid::Uuid>,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
