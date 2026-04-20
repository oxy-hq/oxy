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

#[cfg(test)]
#[path = "types_tests.rs"]
mod types_tests;
