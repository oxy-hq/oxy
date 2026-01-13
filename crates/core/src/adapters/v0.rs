use crate::errors::OxyError;
use reqwest::{
    Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};

const V0_API_BASE_URL: &str = "https://api.v0.dev";

#[derive(Debug, Serialize)]
pub struct CreateChatRequest {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct RepoConfig {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InitChatRequest {
    #[serde(rename = "type")]
    pub init_type: String,
    pub repo: RepoConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateChatResponse {
    pub id: String,
    pub demo: Option<String>,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<LatestVersion>,
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    pub files: Option<Vec<V0FileResponse>>,
    pub messages: Vec<V0Message>,
}

#[derive(Debug, Deserialize)]
pub struct LatestVersion {
    pub id: String,
    #[serde(rename = "demoUrl")]
    pub demo_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct V0FileResponse {
    pub name: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct V0Message {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct V0EnvVar {
    pub key: String,
    pub value: String,
}

impl V0EnvVar {
    pub fn new(key: String, value: String) -> Self {
        Self { key, value }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateEnvironmentRequest {
    #[serde(rename = "environmentVariables")]
    pub environment_variables: Vec<V0EnvVar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upsert: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SendMessageRequest {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
}

pub struct V0Client {
    client: Client,
    api_key: String,
}

impl V0Client {
    pub fn new(api_key: String) -> Result<Self, OxyError> {
        let client = Client::builder()
            .build()
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create HTTP client: {e}")))?;

        Ok(Self { client, api_key })
    }

    fn create_headers(&self) -> Result<HeaderMap, OxyError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key))
                .map_err(|e| OxyError::RuntimeError(format!("Invalid API key format: {e}")))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    pub async fn create_chat(&self, message: String) -> Result<CreateChatResponse, OxyError> {
        let url = format!("{}/v1/chats", V0_API_BASE_URL);
        let headers = self.create_headers()?;

        let request_body = CreateChatRequest { message };

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to create v0 chat: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "v0 API error: {} - {}",
                status, error_text
            )));
        }

        // Debug: print response body
        let response_text = response
            .text()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to read response: {e}")))?;
        println!("v0 API Response: {}", response_text);

        // Parse the text into JSON
        serde_json::from_str::<CreateChatResponse>(&response_text)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse v0 response: {e}")))
    }

    pub async fn init_chat(
        &self,
        github_repo: String,
        name: Option<String>,
    ) -> Result<CreateChatResponse, OxyError> {
        let url: String = format!("{}/v1/chats/init", V0_API_BASE_URL);
        let headers = self.create_headers()?;

        let request_body = InitChatRequest {
            init_type: "repo".to_string(),
            repo: RepoConfig {
                url: github_repo,
                branch: None,
            },
            project_id: None,
            name,
        };

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to init v0 chat from repo: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "v0 API error: {} - {}",
                status, error_text
            )));
        }

        // Debug: print response body
        let response_text = response
            .text()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to read response: {e}")))?;
        println!("v0 API Init Response: {}", response_text);

        // Parse the text into JSON
        serde_json::from_str::<CreateChatResponse>(&response_text)
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse v0 init response: {e}")))
    }

    pub async fn create_environment(
        &self,
        project_id: &str,
        env_vars: Vec<V0EnvVar>,
    ) -> Result<(), OxyError> {
        let url = format!("{}/v1/projects/{}/env-vars", V0_API_BASE_URL, project_id);
        let headers = self.create_headers()?;
        let body = CreateEnvironmentRequest {
            environment_variables: env_vars,
            upsert: Some(true),
        };
        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                OxyError::RuntimeError(format!("Failed to create v0 environment vars: {e}"))
            })?;
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "v0 API error: {} - {}",
                status, error_text
            )));
        }
        Ok(())
    }

    pub async fn send_message(
        &self,
        chat_id: &str,
        message: String,
        system: Option<String>,
    ) -> Result<CreateChatResponse, OxyError> {
        let url = format!("{}/v1/chats/{}/messages", V0_API_BASE_URL, chat_id);
        let headers = self.create_headers()?;

        let request_body = SendMessageRequest { message, system };

        let response = self
            .client
            .post(&url)
            .headers(headers)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to send v0 message: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(OxyError::RuntimeError(format!(
                "v0 API error: {} - {}",
                status, error_text
            )));
        }

        response
            .json::<CreateChatResponse>()
            .await
            .map_err(|e| OxyError::RuntimeError(format!("Failed to parse v0 response: {e}")))
    }
}
