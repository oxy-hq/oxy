use reqwest::{Client, StatusCode};
use serde::Serialize;

use super::error::AirhouseError;
use super::types::{
    CreatedServiceAccount, EphemeralCredential, ServiceAccountRecord, TenantRecord,
    TenantRecordRaw, TokenAuth, UserRecord, UserRole,
};

#[derive(Serialize)]
struct CreateTenantRequest<'a> {
    id: &'a str,
}

#[derive(Serialize)]
struct CreateUserRequest<'a> {
    username: &'a str,
    password: &'a str,
    role: &'a UserRole,
}

#[derive(Serialize)]
struct CreateServiceAccountRequest<'a> {
    name: &'a str,
    tenant_id: &'a str,
    max_role: &'a UserRole,
    max_ttl_secs: i32,
}

#[derive(Serialize)]
struct MintTokenRequest<'a> {
    subject: &'a str,
    role: &'a UserRole,
    ttl_secs: i32,
}

/// Wire shape of the create-SA response. Flattens
/// [`ServiceAccountRecord`] alongside the one-time `bearer` string.
#[derive(serde::Deserialize)]
struct CreateServiceAccountResponse {
    #[serde(flatten)]
    record: ServiceAccountRecord,
    bearer: String,
}

pub struct AirhouseAdminClient {
    client: Client,
    base_url: String,
    token: String,
}

impl AirhouseAdminClient {
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            token: token.into(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/admin/v1{}", self.base_url.trim_end_matches('/'), path)
    }

    /// Create an Airhouse tenant. Storage is managed server-side from
    /// `[storage]` in `airhouse.toml` — the response still surfaces the
    /// resolved `bucket` and `prefix` so callers can persist them locally.
    pub async fn create_tenant(&self, id: &str) -> Result<TenantRecord, AirhouseError> {
        let resp = self
            .client
            .post(self.url("/tenants"))
            .bearer_auth(&self.token)
            .json(&CreateTenantRequest { id })
            .send()
            .await?;
        match resp.status() {
            StatusCode::CREATED => Ok(resp.json::<TenantRecordRaw>().await?.into()),
            StatusCode::BAD_REQUEST => Err(AirhouseError::InvalidInput(resp.text().await?)),
            StatusCode::CONFLICT => Err(AirhouseError::AlreadyExists(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<TenantRecord>, AirhouseError> {
        let resp = self
            .client
            .get(self.url(&format!("/tenants/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(Some(resp.json::<TenantRecordRaw>().await?.into())),
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    pub async fn list_tenants(&self) -> Result<Vec<TenantRecord>, AirhouseError> {
        let resp = self
            .client
            .get(self.url("/tenants"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => {
                let raw: Vec<TenantRecordRaw> = resp.json().await?;
                Ok(raw.into_iter().map(Into::into).collect())
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    /// Delete a tenant. Idempotent — returns `Ok(())` even when the tenant does not exist
    /// because Airhouse returns 204 in both cases.
    pub async fn delete_tenant(&self, id: &str) -> Result<(), AirhouseError> {
        let resp = self
            .client
            .delete(self.url(&format!("/tenants/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    pub async fn create_user(
        &self,
        tenant_id: &str,
        username: &str,
        password: &str,
        role: UserRole,
    ) -> Result<UserRecord, AirhouseError> {
        let resp = self
            .client
            .post(self.url(&format!("/tenants/{tenant_id}/users")))
            .bearer_auth(&self.token)
            .json(&CreateUserRequest {
                username,
                password,
                role: &role,
            })
            .send()
            .await?;
        match resp.status() {
            StatusCode::CREATED => Ok(resp.json::<UserRecord>().await?),
            StatusCode::BAD_REQUEST => Err(AirhouseError::InvalidInput(resp.text().await?)),
            StatusCode::CONFLICT => Err(AirhouseError::AlreadyExists(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    pub async fn list_users(&self, tenant_id: &str) -> Result<Vec<UserRecord>, AirhouseError> {
        let resp = self
            .client
            .get(self.url(&format!("/tenants/{tenant_id}/users")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json::<Vec<UserRecord>>().await?),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    /// Delete a user. Returns `true` on success, `false` if the user does not exist (404).
    pub async fn delete_user(
        &self,
        tenant_id: &str,
        username: &str,
    ) -> Result<bool, AirhouseError> {
        let resp = self
            .client
            .delete(self.url(&format!("/tenants/{tenant_id}/users/{username}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    // ── Service accounts ──────────────────────────────────────────────────────

    /// Create a service account. The returned `bearer` is shown **once** —
    /// Airhouse persists only its hash. Store it via your secret manager
    /// immediately; subsequent `GET` does not return it.
    pub async fn create_service_account(
        &self,
        name: &str,
        tenant_id: &str,
        max_role: UserRole,
        max_ttl_secs: i32,
    ) -> Result<CreatedServiceAccount, AirhouseError> {
        let resp = self
            .client
            .post(self.url("/service-accounts"))
            .bearer_auth(&self.token)
            .json(&CreateServiceAccountRequest {
                name,
                tenant_id,
                max_role: &max_role,
                max_ttl_secs,
            })
            .send()
            .await?;
        match resp.status() {
            StatusCode::CREATED => {
                let body: CreateServiceAccountResponse = resp.json().await?;
                Ok(CreatedServiceAccount {
                    record: body.record,
                    bearer: body.bearer,
                })
            }
            StatusCode::BAD_REQUEST => Err(AirhouseError::InvalidInput(resp.text().await?)),
            StatusCode::UNAUTHORIZED => Err(AirhouseError::Unauthorized(resp.text().await?)),
            StatusCode::NOT_FOUND => Err(AirhouseError::NotFound(resp.text().await?)),
            StatusCode::CONFLICT => Err(AirhouseError::AlreadyExists(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    pub async fn list_service_accounts(&self) -> Result<Vec<ServiceAccountRecord>, AirhouseError> {
        let resp = self
            .client
            .get(self.url("/service-accounts"))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::OK => Ok(resp.json::<Vec<ServiceAccountRecord>>().await?),
            StatusCode::UNAUTHORIZED => Err(AirhouseError::Unauthorized(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    /// Revoke a service account. Idempotent — Airhouse returns 204 whether or
    /// not the SA existed. Outstanding ephemeral credentials minted by this
    /// SA continue to authenticate until their own `expires_at`.
    pub async fn revoke_service_account(&self, id: &str) -> Result<(), AirhouseError> {
        let resp = self
            .client
            .delete(self.url(&format!("/service-accounts/{id}")))
            .bearer_auth(&self.token)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::UNAUTHORIZED => Err(AirhouseError::Unauthorized(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    // ── Tokens ────────────────────────────────────────────────────────────────

    /// Mint a short-lived ephemeral wire-protocol credential against `tenant_id`.
    /// Authenticated as the SA whose bearer is `sa_bearer` — the SA must be
    /// bound to `tenant_id`, its `max_role` must cover the requested `role`,
    /// and its `max_ttl_secs` must cover `ttl_secs`.
    pub async fn mint_token(
        &self,
        tenant_id: &str,
        sa_bearer: &str,
        subject: &str,
        role: UserRole,
        ttl_secs: i32,
    ) -> Result<EphemeralCredential, AirhouseError> {
        let resp = self
            .client
            .post(self.url(&format!("/tenants/{tenant_id}/tokens")))
            .bearer_auth(sa_bearer)
            .json(&MintTokenRequest {
                subject,
                role: &role,
                ttl_secs,
            })
            .send()
            .await?;
        match resp.status() {
            StatusCode::CREATED => Ok(resp.json::<EphemeralCredential>().await?),
            StatusCode::BAD_REQUEST => Err(AirhouseError::InvalidInput(resp.text().await?)),
            StatusCode::UNAUTHORIZED => Err(AirhouseError::Unauthorized(resp.text().await?)),
            StatusCode::FORBIDDEN => Err(AirhouseError::Forbidden(resp.text().await?)),
            StatusCode::TOO_MANY_REQUESTS => Err(AirhouseError::RateLimited(resp.text().await?)),
            StatusCode::SERVICE_UNAVAILABLE => Err(AirhouseError::Provisioning(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }

    /// Revoke a single ephemeral credential. Returns `true` on 204, `false` on
    /// 404 (already gone or never existed). Existing in-flight pgwire
    /// connections using this credential are NOT terminated; only future SCRAM
    /// auth attempts fail.
    ///
    /// `auth` selects between the deployment-wide admin token and the issuing
    /// SA's bearer. An SA bearer that did not issue this credential gets 403.
    pub async fn revoke_token(
        &self,
        tenant_id: &str,
        username: &str,
        auth: TokenAuth<'_>,
    ) -> Result<bool, AirhouseError> {
        let bearer = match auth {
            TokenAuth::Admin => self.token.as_str(),
            TokenAuth::ServiceAccount(b) => b,
        };
        let resp = self
            .client
            .delete(self.url(&format!("/tenants/{tenant_id}/tokens/{username}")))
            .bearer_auth(bearer)
            .send()
            .await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(true),
            StatusCode::NOT_FOUND => Ok(false),
            StatusCode::UNAUTHORIZED => Err(AirhouseError::Unauthorized(resp.text().await?)),
            StatusCode::FORBIDDEN => Err(AirhouseError::Forbidden(resp.text().await?)),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(AirhouseError::Provisioning(resp.text().await?))
            }
            s => Err(AirhouseError::Provisioning(format!(
                "unexpected status {s}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn tenant_json() -> serde_json::Value {
        json!({
            "id": "acme",
            "pg_url": "postgres:dbname=acme host=catalog port=5433 user=airhouse_tenant_acme password=secret",
            "bucket": "airhouse-data",
            "prefix": "tenants/acme",
            "role": "airhouse_tenant_acme",
            "status": "active",
            "created_at": "2026-04-29T10:00:00Z"
        })
    }

    fn user_json() -> serde_json::Value {
        json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "tenant_id": "acme",
            "username": "alice",
            "role": "reader",
            "created_at": "2026-04-29T10:01:00Z"
        })
    }

    #[tokio::test]
    async fn test_create_tenant_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&tenant_json()))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let rec = client.create_tenant("acme").await.unwrap();
        assert_eq!(rec.id, "acme");
        // Bucket + prefix come back from the server, not from the request body.
        assert_eq!(rec.bucket, "airhouse-data");
        assert!(!rec.pg_url().is_empty());
    }

    #[tokio::test]
    async fn test_create_tenant_conflict_maps_to_already_exists() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants"))
            .respond_with(ResponseTemplate::new(409).set_body_string("tenant already exists"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client.create_tenant("acme").await.unwrap_err();
        assert!(matches!(err, AirhouseError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn test_create_tenant_500_maps_to_provisioning() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants"))
            .respond_with(ResponseTemplate::new(500).set_body_string("catalog DB misconfigured"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client.create_tenant("acme").await.unwrap_err();
        assert!(matches!(err, AirhouseError::Provisioning(_)));
    }

    #[tokio::test]
    async fn test_delete_tenant_idempotent() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/tenants/acme"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        // Airhouse always returns 204, even when tenant did not exist.
        assert!(client.delete_tenant("acme").await.is_ok());
    }

    #[tokio::test]
    async fn test_create_user_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/users"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&user_json()))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let rec = client
            .create_user("acme", "alice", "s3cr3t", UserRole::Reader)
            .await
            .unwrap();
        assert_eq!(rec.username, "alice");
        assert_eq!(rec.tenant_id, "acme");
    }

    #[tokio::test]
    async fn test_delete_user_not_found_returns_false() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/tenants/acme/users/alice"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let deleted = client.delete_user("acme", "alice").await.unwrap();
        assert!(!deleted);
    }

    // ── Service accounts ──────────────────────────────────────────────────────

    fn sa_record_json() -> serde_json::Value {
        json!({
            "id": "sa_8f2a8c10ab12cd34",
            "name": "oxy-tenant-acme",
            "tenant_id": "acme",
            "max_role": "admin",
            "max_ttl_secs": 86400,
            "created_at": "2026-05-05T12:00:00Z",
            "revoked_at": null,
            "last_used_at": null
        })
    }

    fn create_sa_response_json() -> serde_json::Value {
        let mut v = sa_record_json();
        v.as_object_mut().unwrap().insert(
            "bearer".to_string(),
            json!("ahsa_3e8b1f4d2c5a6b7e8d9f0a1b2c3d4e5f"),
        );
        v
    }

    #[tokio::test]
    async fn test_create_service_account_success_returns_one_time_bearer() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/service-accounts"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&create_sa_response_json()))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let created = client
            .create_service_account("oxy-tenant-acme", "acme", UserRole::Admin, 86400)
            .await
            .unwrap();
        assert_eq!(created.record.id, "sa_8f2a8c10ab12cd34");
        assert_eq!(created.record.tenant_id, "acme");
        assert_eq!(created.record.max_role, "admin");
        assert_eq!(created.record.max_ttl_secs, 86400);
        assert!(created.bearer.starts_with("ahsa_"));
    }

    #[tokio::test]
    async fn test_create_service_account_400_maps_to_invalid_input() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/service-accounts"))
            .respond_with(ResponseTemplate::new(400).set_body_string("name is required"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .create_service_account("", "acme", UserRole::Admin, 86400)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_create_service_account_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/service-accounts"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "wrong");
        let err = client
            .create_service_account("oxy-tenant-acme", "acme", UserRole::Admin, 86400)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn test_create_service_account_404_maps_to_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/service-accounts"))
            .respond_with(ResponseTemplate::new(404).set_body_string("tenant not found"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .create_service_account("oxy-tenant-acme", "missing", UserRole::Admin, 86400)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_list_service_accounts_returns_array() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/admin/v1/service-accounts"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json!([sa_record_json()])))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let rows = client.list_service_accounts().await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "sa_8f2a8c10ab12cd34");
    }

    #[tokio::test]
    async fn test_revoke_service_account_idempotent() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/service-accounts/sa_8f2a8c10ab12cd34"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        // Idempotent — Airhouse returns 204 whether or not the SA existed.
        assert!(
            client
                .revoke_service_account("sa_8f2a8c10ab12cd34")
                .await
                .is_ok()
        );
    }

    // ── Token mint + revoke ───────────────────────────────────────────────────

    fn mint_response_json() -> serde_json::Value {
        json!({
            "username": "eph_a1b2c3d4ef98",
            "password": "tk_3e8b1f4d2c5a6b7e8d9f0a1b2c3d4e5f",
            "tenant": "acme",
            "role": "reader",
            "expires_at": "2026-05-05T12:15:00Z",
            "service_account_id": "sa_8f2a8c10ab12cd34"
        })
    }

    #[tokio::test]
    async fn test_mint_token_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/tokens"))
            .respond_with(ResponseTemplate::new(201).set_body_json(&mint_response_json()))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let cred = client
            .mint_token("acme", "ahsa_secret", "user_alice", UserRole::Reader, 900)
            .await
            .unwrap();
        assert!(cred.username.starts_with("eph_"));
        assert!(cred.password.starts_with("tk_"));
        assert_eq!(cred.tenant, "acme");
        assert_eq!(cred.service_account_id, "sa_8f2a8c10ab12cd34");
    }

    #[tokio::test]
    async fn test_mint_token_429_maps_to_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/tokens"))
            .respond_with(ResponseTemplate::new(429).set_body_string("mint rate limit exceeded"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .mint_token("acme", "ahsa_secret", "user_alice", UserRole::Reader, 900)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::RateLimited(_)));
    }

    #[tokio::test]
    async fn test_mint_token_403_maps_to_forbidden() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/tokens"))
            .respond_with(ResponseTemplate::new(403).set_body_string(
                "requested ttl_secs 7200 exceeds service-account max_ttl_secs 3600",
            ))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .mint_token("acme", "ahsa_secret", "user_alice", UserRole::Admin, 7200)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::Forbidden(_)));
    }

    #[tokio::test]
    async fn test_mint_token_401_maps_to_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/tokens"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .mint_token("acme", "ahsa_revoked", "user_alice", UserRole::Reader, 900)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::Unauthorized(_)));
    }

    #[tokio::test]
    async fn test_mint_token_400_maps_to_invalid_input() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/admin/v1/tenants/acme/tokens"))
            .respond_with(ResponseTemplate::new(400).set_body_string("ttl_secs must be positive"))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .mint_token("acme", "ahsa_secret", "user_alice", UserRole::Reader, 0)
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_revoke_token_204_returns_true() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/tenants/acme/tokens/eph_a1b2c3d4ef98"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let revoked = client
            .revoke_token("acme", "eph_a1b2c3d4ef98", TokenAuth::Admin)
            .await
            .unwrap();
        assert!(revoked);
    }

    #[tokio::test]
    async fn test_revoke_token_404_returns_false() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/tenants/acme/tokens/eph_missing"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let revoked = client
            .revoke_token("acme", "eph_missing", TokenAuth::Admin)
            .await
            .unwrap();
        assert!(!revoked);
    }

    #[tokio::test]
    async fn test_revoke_token_403_when_sa_did_not_issue() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/admin/v1/tenants/acme/tokens/eph_a1b2c3d4ef98"))
            .respond_with(
                ResponseTemplate::new(403)
                    .set_body_string("service account 'sa_other' did not issue this credential"),
            )
            .mount(&server)
            .await;

        let client = AirhouseAdminClient::new(server.uri(), "tok");
        let err = client
            .revoke_token(
                "acme",
                "eph_a1b2c3d4ef98",
                TokenAuth::ServiceAccount("ahsa_other_sa"),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AirhouseError::Forbidden(_)));
    }
}
