use axum::extract::State;
use axum::{
    extract,
    http::{HeaderMap, StatusCode},
    response::Json,
};
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Utc};
use entity::{prelude::Users, users};
use jsonwebtoken::{EncodingKey, Header, encode};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::types::AuthMode;
use crate::{
    config::{ConfigBuilder, constants::AUTHENTICATION_SECRET_KEY},
    db::client::establish_connection,
    errors::OxyError,
    utils::find_project_path,
};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct GoogleAuthRequest {
    pub code: String,
}

#[derive(Deserialize)]
pub struct ValidateEmailRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub picture: Option<String>,
}

#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    email: String,
    exp: usize,
    iat: usize,
}

#[derive(Serialize)]
pub struct AuthConfigResponse {
    pub is_built_in_mode: bool,
    pub auth_enabled: bool,
    pub google: Option<GoogleConfig>,
    pub basic: Option<bool>,
}

#[derive(Serialize)]
pub struct GoogleConfig {
    pub client_id: String,
}

pub async fn get_config(
    State(auth_mode): State<AuthMode>,
) -> Result<Json<AuthConfigResponse>, StatusCode> {
    if auth_mode != AuthMode::BuiltIn {
        return Ok(Json(AuthConfigResponse {
            is_built_in_mode: false,
            auth_enabled: false,
            google: None,
            basic: None,
        }));
    }

    let project_path = find_project_path().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .build()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let auth_config = config.get_authentication();
    if auth_config.is_none() {
        return Ok(Json(AuthConfigResponse {
            is_built_in_mode: true,
            auth_enabled: false,
            google: None,
            basic: None,
        }));
    }
    let google_client_id = auth_config
        .as_ref()
        .and_then(|auth| auth.google.as_ref())
        .map(|google| google.client_id.clone());
    let basic_auth_enabled = auth_config
        .as_ref()
        .and_then(|auth| auth.basic.as_ref())
        .is_some();

    let config = AuthConfigResponse {
        is_built_in_mode: true,
        auth_enabled: true,
        google: google_client_id.map(|client_id| GoogleConfig { client_id }),
        basic: Some(basic_auth_enabled),
    };

    Ok(Json(config))
}

pub async fn login(
    extract::Json(login_request): extract::Json<LoginRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let connection = establish_connection().await;

    let user = Users::find()
        .filter(users::Column::Email.eq(&login_request.email))
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let empty_string = String::new();
    let password_hash = user.password_hash.as_ref().unwrap_or(&empty_string);
    if !verify_password(&login_request.password, password_hash) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if !user.email_verified {
        return Err(StatusCode::FORBIDDEN);
    }

    let token = create_auth_token(user.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let auth_response = AuthResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
        },
    };

    Ok(Json(auth_response))
}

pub async fn create_auth_token(user: users::Model) -> Result<String, StatusCode> {
    let connection = establish_connection().await;

    let user_clone = user.clone();
    let mut user_update: users::ActiveModel = user.into();
    user_update.last_login_at = Set(chrono::Utc::now().into());
    user_update.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to update user last login: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let now = Utc::now();
    let exp = now + Duration::weeks(1);

    let claims = Claims {
        sub: user_clone.id.to_string(),
        email: user_clone.email.clone(),
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(AUTHENTICATION_SECRET_KEY.as_bytes()),
    )
    .map_err(|e| {
        tracing::error!("Failed to generate JWT token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(token)
}

pub async fn register(
    headers: HeaderMap,
    extract::Json(register_request): extract::Json<RegisterRequest>,
) -> Result<Json<MessageResponse>, StatusCode> {
    let connection = establish_connection().await;

    let existing_user = Users::find()
        .filter(users::Column::Email.eq(&register_request.email))
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query existing user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if existing_user.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let password_hash = hash_password(&register_request.password);

    let verification_token = Uuid::new_v4().to_string();

    let new_user = users::ActiveModel {
        id: Set(Uuid::new_v4()),
        email: Set(register_request.email.clone()),
        name: Set(register_request.name),
        picture: Set(None),
        password_hash: Set(Some(password_hash)),
        email_verified: Set(false),
        email_verification_token: Set(Some(verification_token.clone())),
        created_at: sea_orm::ActiveValue::NotSet,
        last_login_at: sea_orm::ActiveValue::NotSet,
    };

    let user = new_user.insert(&connection).await.map_err(|e| {
        tracing::error!("Failed to create user: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let email = register_request.email.clone();
    let token = verification_token.clone();
    tokio::spawn(async move {
        let base_url = extract_base_url_from_headers(&headers);
        if let Err(e) = send_verification_email(&email, &token, &base_url).await {
            tracing::error!("Failed to send verification email: {}", e);
        }
    });

    tracing::info!("Created new user: {} ({})", user.email, user.id);

    Ok(Json(MessageResponse {
        message: "User registered successfully. Please check your email for verification."
            .to_string(),
    }))
}

pub async fn google_auth(
    headers: HeaderMap,
    extract::Json(google_request): extract::Json<GoogleAuthRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let base_url = extract_base_url_from_headers(&headers);
    let user_info = exchange_google_code_for_user_info(&google_request.code, &base_url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to exchange Google code: {}", e);
            StatusCode::UNAUTHORIZED
        })?;

    let connection = establish_connection().await;

    let user = match Users::find()
        .filter(users::Column::Email.eq(&user_info.email))
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        Some(existing_user) => {
            let mut user_update: users::ActiveModel = existing_user.clone().into();
            user_update.name = Set(user_info.name.clone());
            user_update.picture = Set(user_info.picture.clone());
            user_update.email_verified = Set(true);
            user_update.last_login_at = Set(chrono::Utc::now().into());
            user_update.update(&connection).await.map_err(|e| {
                tracing::error!("Failed to update user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
        None => {
            let new_user = users::ActiveModel {
                id: Set(Uuid::new_v4()),
                email: Set(user_info.email.clone()),
                name: Set(user_info.name.clone()),
                picture: Set(user_info.picture.clone()),
                password_hash: Set(None),
                email_verified: Set(true),
                email_verification_token: Set(None),
                created_at: sea_orm::ActiveValue::NotSet,
                last_login_at: sea_orm::ActiveValue::NotSet,
            };

            new_user.insert(&connection).await.map_err(|e| {
                tracing::error!("Failed to create user: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
    };

    let token = create_auth_token(user.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let auth_response = AuthResponse {
        token,
        user: UserInfo {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            picture: user.picture,
        },
    };

    Ok(Json(auth_response))
}

pub async fn validate_email(
    extract::Json(validate_request): extract::Json<ValidateEmailRequest>,
) -> Result<Json<AuthResponse>, StatusCode> {
    let connection = establish_connection().await;

    let user = Users::find()
        .filter(users::Column::EmailVerificationToken.eq(&validate_request.token))
        .one(&connection)
        .await
        .map_err(|e| {
            tracing::error!("Failed to query user by verification token: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let user_clone = user.clone();

    let mut user_update: users::ActiveModel = user.into();
    user_update.email_verified = Set(true);
    user_update.email_verification_token = Set(None);
    user_update.update(&connection).await.map_err(|e| {
        tracing::error!("Failed to update user email verification: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let token = create_auth_token(user_clone.clone()).await.map_err(|e| {
        tracing::error!("Failed to create auth token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let auth_response = AuthResponse {
        token,
        user: UserInfo {
            id: user_clone.id.to_string(),
            email: user_clone.email,
            name: user_clone.name,
            picture: user_clone.picture,
        },
    };
    Ok(Json(auth_response))
}

fn hash_password(password: &str) -> String {
    hash(password, DEFAULT_COST).expect("Failed to hash password")
}

fn verify_password(password: &str, hash: &str) -> bool {
    verify(password, hash).unwrap_or(false)
}

fn extract_base_url_from_headers(headers: &HeaderMap) -> String {
    let scheme = if headers.get("x-forwarded-proto").is_some() {
        headers
            .get("x-forwarded-proto")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("https")
    } else if headers.get("x-forwarded-ssl").is_some() {
        "https"
    } else {
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost");

        if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
            "http"
        } else {
            "https"
        }
    };

    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:5173");

    format!("{}://{}", scheme, host)
}

async fn send_verification_email(email: &str, token: &str, base_url: &str) -> Result<(), OxyError> {
    let project_path = find_project_path()
        .map_err(|_| OxyError::ConfigurationError("Failed to find project path".to_owned()))?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?
        .build()
        .await
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?;

    let auth_config = config.get_authentication();

    if let Some(auth) = auth_config {
        if let Some(basic_auth) = &auth.basic {
            let verification_url = format!("{}/verify-email?token={}", base_url, token);

            let email_body = format!(
                "Welcome to Onyx!\n\nPlease verify your email address by clicking the link below:\n\n{}\n\nIf you didn't create an account, please ignore this email.",
                verification_url
            );

            let email_message = Message::builder()
                .from(basic_auth.smtp_user.parse().map_err(|e| {
                    OxyError::ConfigurationError(format!("Invalid from email: {}", e))
                })?)
                .to(email.parse().map_err(|e| {
                    OxyError::ConfigurationError(format!("Invalid to email: {}", e))
                })?)
                .subject("Verify your email address")
                .body(email_body)
                .map_err(|e| {
                    OxyError::ConfigurationError(format!("Failed to build email: {}", e))
                })?;

            let smtp_password =
                std::env::var(basic_auth.smtp_password_var.as_str()).map_err(|e| {
                    OxyError::ConfigurationError(format!(
                        "SMTP password key not found in environment variable {}:\n{}",
                        basic_auth.smtp_password_var, e
                    ))
                })?;

            let creds = Credentials::new(basic_auth.smtp_user.clone(), smtp_password.clone());

            let smtp_server = basic_auth
                .smtp_server
                .as_deref()
                .unwrap_or("smtp.gmail.com");
            let smtp_port = basic_auth.smtp_port.unwrap_or(587);

            let mailer = SmtpTransport::starttls_relay(smtp_server)
                .map_err(|e| {
                    OxyError::ConfigurationError(format!("Failed to connect to SMTP server: {}", e))
                })?
                .credentials(creds)
                .port(smtp_port)
                .build();

            mailer.send(&email_message).map_err(|e| {
                OxyError::ConfigurationError(format!("Failed to send email: {}", e))
            })?;

            tracing::info!("Verification email sent to {}", email);
        } else {
            tracing::warn!("No SMTP configuration found");
        }
    } else {
        tracing::warn!("No authentication configuration found");
    }

    Ok(())
}

#[derive(Deserialize)]
struct GoogleUserInfo {
    email: String,
    name: String,
    picture: Option<String>,
}

async fn exchange_google_code_for_user_info(
    code: &str,
    base_url: &str,
) -> Result<GoogleUserInfo, OxyError> {
    let project_path = find_project_path()
        .map_err(|_| OxyError::ConfigurationError("Failed to find project path".to_owned()))?;

    let config = ConfigBuilder::new()
        .with_project_path(&project_path)
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?
        .build()
        .await
        .map_err(|_| OxyError::ConfigurationError("Failed to build config".to_owned()))?;

    let auth_config = config.get_authentication();

    let google_config = auth_config.and_then(|auth| auth.google).ok_or_else(|| {
        OxyError::ConfigurationError("Google OAuth configuration not found".to_string())
    })?;

    let client = reqwest::Client::new();

    let redirect_uri = format!("{}/auth/google/callback", base_url);

    println!("Redirect URI: {}", redirect_uri);

    let client_secret = std::env::var(google_config.client_secret_var.as_str()).map_err(|e| {
        OxyError::ConfigurationError(format!(
            "Google client secret key not found in environment variable {}:\n{}",
            google_config.client_secret_var, e
        ))
    })?;

    let token_request = serde_json::json!({
        "client_id": google_config.client_id,
        "client_secret": client_secret,
        "code": code,
        "grant_type": "authorization_code",
        "redirect_uri": redirect_uri
    });

    let token_response = client
        .post("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/json")
        .json(&token_request)
        .send()
        .await
        .map_err(|e| {
            OxyError::ConfigurationError(format!("Failed to exchange code for token: {}", e))
        })?;

    let token_data: serde_json::Value = token_response.json().await.map_err(|e| {
        OxyError::ConfigurationError(format!("Failed to parse token response: {}", e))
    })?;

    let access_token = token_data["access_token"]
        .as_str()
        .ok_or_else(|| OxyError::ConfigurationError("No access token in response".to_string()))?;

    let user_info_response = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| OxyError::ConfigurationError(format!("Failed to get user info: {}", e)))?;

    let user_info: GoogleUserInfo = user_info_response
        .json()
        .await
        .map_err(|e| OxyError::ConfigurationError(format!("Failed to parse user info: {}", e)))?;

    tracing::info!(
        "Successfully exchanged Google authorization code for user: {}",
        user_info.email
    );

    Ok(user_info)
}
