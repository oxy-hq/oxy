use crate::errors::OxyError;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::{
    Client,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use std::env;
use tracing::info;

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AppTokenClaims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct InstallationTokenResponse {
    token: String,
    expires_at: String,
}

pub struct GitHubAppAuth {
    app_id: String,
    private_key: String,
    client: Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubAccount {
    login: String,
    #[serde(rename = "type")]
    account_type: String,
    id: i64,
    // Other fields available but not needed for our use case
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubInstallationResponse {
    id: i64,
    account: GitHubAccount,
    app_slug: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitHubInstallation {
    pub id: i64,
    pub owner_type: String,
    pub slug: String,
    pub name: String,
}

impl GitHubAppAuth {
    pub fn new(app_id: String, private_key: String) -> Result<Self, OxyError> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("Oxy-GitHub-App/1.0"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github.v3+json"),
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self {
            app_id,
            private_key,
            client,
            base_url: "https://api.github.com".to_string(),
        })
    }

    pub fn from_env() -> Result<Self, OxyError> {
        let app_id = env::var("GITHUB_APP_ID").map_err(|_| {
            OxyError::ConfigurationError("GitHub App ID not configured in environment".to_string())
        })?;

        let private_key = env::var("GITHUB_APP_PRIVATE_KEY").map_err(|_| {
            OxyError::ConfigurationError(
                "GitHub App private key not configured in environment".to_string(),
            )
        })?;

        Self::new(app_id, private_key)
    }

    pub async fn get_user_oauth_token(&self, code: &str) -> Result<String, OxyError> {
        let client_id = env::var("GITHUB_CLIENT_ID").map_err(|_| {
            OxyError::ConfigurationError(
                "GitHub Client ID not configured in environment".to_string(),
            )
        })?;

        let client_secret = env::var("GITHUB_CLIENT_SECRET").map_err(|_| {
            OxyError::ConfigurationError(
                "GitHub Client Secret not configured in environment".to_string(),
            )
        })?;

        let url = format!("{}/login/oauth/access_token", "https://github.com");
        let params = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", code),
        ];

        let response = self
            .client
            .post(url)
            .form(&params)
            .header(ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to exchange OAuth code for token: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub OAuth error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let token_response: OAuthTokenResponse = response.json().await.map_err(|e| {
            OxyError::RuntimeError(format!("Failed to parse OAuth token response: {e}"))
        })?;

        Ok(token_response.access_token)
    }

    pub async fn get_installation_info(
        &self,
        installation_id: &str,
    ) -> Result<GitHubInstallation, OxyError> {
        let jwt = self.generate_jwt()?;

        let url = format!("{}/app/installations/{}", self.base_url, installation_id);
        let response = self
            .client
            .get(&url)
            .header(AUTHORIZATION, format!("Bearer {}", jwt))
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to get installation info: {e}")))?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let installation_response: GitHubInstallationResponse =
            response.json().await.map_err(|e| {
                OxyError::RuntimeError(format!("Failed to parse installation info: {e}"))
            })?;

        let installation = GitHubInstallation {
            id: installation_response.id,
            owner_type: installation_response.account.account_type,
            slug: installation_response.app_slug,
            name: installation_response.account.login,
        };

        Ok(installation)
    }

    pub fn generate_jwt(&self) -> Result<String, OxyError> {
        let now = Utc::now();
        let iat = now.timestamp();
        let exp = (now + Duration::minutes(10)).timestamp();

        let claims = AppTokenClaims {
            iat,
            exp,
            iss: self.app_id.clone(),
        };

        let header = Header::new(Algorithm::RS256);
        let key = EncodingKey::from_rsa_pem(self.private_key.as_bytes())
            .map_err(|e| OxyError::RuntimeError(format!("Invalid private key: {e}")))?;

        encode(&header, &claims, &key)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to generate JWT: {e}")))
    }

    pub async fn get_installation_token(&self, installation_id: &str) -> Result<String, OxyError> {
        info!(
            "Getting installation token for installation ID: {}",
            installation_id
        );
        let jwt = self.generate_jwt()?;

        let url = format!(
            "{}/app/installations/{}/access_tokens",
            self.base_url, installation_id
        );

        let response = self
            .client
            .post(&url)
            .header(AUTHORIZATION, format!("Bearer {}", jwt))
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to get installation token: {e}"))
            })?;

        if !response.status().is_success() {
            return Err(OxyError::RuntimeError(format!(
                "GitHub API error: {} - {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let token_response: InstallationTokenResponse = response
            .json()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse token response: {e}")))?;

        info!(
            "Successfully obtained installation token, expires at: {}",
            token_response.expires_at
        );
        Ok(token_response.token)
    }
}
