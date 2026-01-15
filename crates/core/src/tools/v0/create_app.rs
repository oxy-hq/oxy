use crate::{
    adapters::v0::{V0Client, V0EnvVar},
    execute::{
        Executable, ExecutionContext,
        types::{Output, event::SandboxAppKind},
    },
};
use oxy_shared::errors::OxyError;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

pub struct CreateV0App;

#[derive(Debug, Clone, Deserialize)]
pub struct CreateV0AppInput {
    pub name: Option<String>,
    pub prompt: String,
    pub system_instruction: String,
    pub github_repo: Option<String>,
    pub oxy_api_key: Option<SecretString>,
    pub v0_api_key: SecretString,
}

#[async_trait::async_trait]
impl Executable<CreateV0AppInput> for CreateV0App {
    type Response = Output;

    async fn execute(
        &mut self,
        execution_context: &ExecutionContext,
        input: CreateV0AppInput,
    ) -> Result<Self::Response, OxyError> {
        tracing::info!("Creating v0 app with input: {:?}", &input);

        // Get API key from environment
        let api_key = input.v0_api_key.expose_secret().to_string();
        let v0_client = V0Client::new(api_key)?;

        // Try to get chat_id from input, ExecutionContext sandbox_info, or create new
        let chat_id = execution_context
            .sandbox_info
            .as_ref()
            .and_then(|info| match &info.kind {
                crate::execute::types::event::SandboxAppKind::V0 { chat_id } => {
                    Some(chat_id.clone())
                }
            });

        // Either continue existing chat or create new one
        let chat_response = if let Some(ref chat_id_ref) = chat_id {
            tracing::info!("Continuing existing v0 chat with ID: {}", chat_id_ref);
            v0_client
                .send_message(chat_id_ref, input.prompt, None)
                .await?
        } else if let Some(ref github_repo) = input.github_repo {
            tracing::info!("Initializing new v0 chat from GitHub repo: {}", github_repo);
            let init_response = v0_client
                .init_chat(github_repo.clone(), input.name.clone())
                .await?;
            if let Some(preview_url) = init_response
                .latest_version
                .as_ref()
                .and_then(|version| version.demo_url.clone())
            {
                tracing::info!("Initialized v0 chat preview URL: {}", preview_url);
                execution_context
                    .write_create_sandbox_app(
                        SandboxAppKind::V0 {
                            chat_id: init_response.id.clone(),
                        },
                        preview_url.clone(),
                    )
                    .await?;
            }
            if let Some(project_id) = init_response.project_id {
                tracing::info!("Initialized v0 chat linked to project ID: {}", project_id);
                let oxy_url = std::env::var("OXY_API_URL")
                    .unwrap_or_else(|_| "http://127.0.0.1:3000/api".to_string());
                let mut env_vars = vec![
                    V0EnvVar::new("OXY_URL".to_string(), oxy_url),
                    V0EnvVar::new(
                        "OXY_PROJECT_ID".to_string(),
                        execution_context.project.project_id.to_string(),
                    ),
                ];
                if let Some(oxy_api_key) = input.oxy_api_key {
                    env_vars.push(V0EnvVar::new(
                        "OXY_API_KEY".to_string(),
                        oxy_api_key.expose_secret().to_string(),
                    ));
                }
                v0_client.create_environment(&project_id, env_vars).await?;
            }
            // After initializing from repo, send the prompt as a follow-up message
            v0_client
                .send_message(
                    &init_response.id,
                    input.prompt,
                    Some(input.system_instruction.clone()),
                )
                .await?
        } else {
            tracing::info!("Creating new v0 chat");
            v0_client.create_chat(input.prompt).await?
        };
        let mut context = chat_response
            .messages
            .last()
            .ok_or_else(|| {
                OxyError::RuntimeError("v0 chat response contains no messages".to_string())
            })?
            .content
            .clone();

        if let Some(preview_url) = chat_response
            .latest_version
            .as_ref()
            .and_then(|version| version.demo_url.clone())
        {
            tracing::info!("v0 chat preview URL: {}", preview_url);
            execution_context
                .write_create_sandbox_app(
                    SandboxAppKind::V0 {
                        chat_id: chat_response.id.clone(),
                    },
                    preview_url.clone(),
                )
                .await?;
            context = format!("{}\n\nPreview your app here: {}", context, preview_url);
        }

        let action = if chat_id.is_some() {
            "updated"
        } else {
            "created"
        };

        Ok(Output::Text(format!(
            "v0 app {} successfully!\nLast Message: {}",
            action, context
        )))
    }
}
