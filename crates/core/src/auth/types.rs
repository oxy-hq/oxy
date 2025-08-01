use clap::ValueEnum;
use entity::users::{self, UserRole, UserStatus};

#[derive(Debug, Clone, ValueEnum, PartialEq, Copy)]
pub enum AuthMode {
    // Use Google IAP for authentication
    IAP,
    // Use trusted Cloud Run identity headers for authentication
    IAPCloudRun,
    // Use Amazon Cognito for authentication (support ALB + congito setup)
    Cognito,
    // Use build-in authentication (default)
    BuiltIn,
}

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
