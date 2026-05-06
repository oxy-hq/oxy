use reqwest::{Client, StatusCode};
use serde::Serialize;

use super::error::AirhouseError;
use super::types::{TenantRecord, TenantRecordRaw, UserRecord, UserRole};

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
}
