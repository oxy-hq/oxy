use entity::users::{self, UserRole, UserStatus};

// `Auth` marker type removed — routes use unit `()` state instead.

// Simple identity structure for email-based identity linking
#[derive(Debug, Clone)]
pub struct Identity {
    pub idp_id: Option<String>, // Identity from the identity provider (e.g., Google IAP)
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
    pub role: UserRole,
    pub status: UserStatus,
}

impl From<users::Model> for AuthenticatedUser {
    fn from(user: users::Model) -> Self {
        Self {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role,
            status: user.status,
        }
    }
}
