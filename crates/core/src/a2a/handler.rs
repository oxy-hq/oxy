//! Oxy implementation of the A2A handler trait.
//!
//! This module provides the `OxyA2aHandler` struct which implements the `A2aHandler`
//! trait from the `a2a` crate. Each handler instance is scoped to a specific agent
//! and handles all A2A protocol operations for that agent.
//!
//! # Architecture
//!
//! - **Agent Scoping**: Each handler instance is created for a specific agent
//!   via the `agent_name` field. This agent name determines which Oxy agent
//!   configuration to load and execute.
//! - **Factory Pattern**: Use `new()` to create agent-scoped instances.
//! - **Storage Integration**: Uses agent-scoped `TaskStorage` for task persistence.
//! - **Agent Execution**: Leverages existing Oxy `AgentLauncher` for execution.
//!
//! # Example
//!
//! ```rust,ignore
//! use oxy_core::a2a::handler::OxyA2aHandler;
//!
//! // Create handler for "sales-assistant" agent
//! let handler = OxyA2aHandler::new(
//!     "sales-assistant".to_string(),
//!     config.clone(),
//!     storage.clone(),
//!     project_manager.clone(),
//! );
//!
//! // Handler will automatically load and execute sales-assistant agent
//! let task = handler.handle_send_message(ctx, message).await?;
//! ```

use a2a::{
    error::A2aError,
    server::{A2aContext, A2aHandler, SseStream},
    storage::TaskStorage,
    types::{AgentCard, Message, Task},
};
use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

use crate::auth::api_key::authenticate_header_with_config;
use crate::config::constants::DEFAULT_API_KEY_HEADER;
use crate::service::api_key::ApiKeyConfig;
use crate::{adapters::project::manager::ProjectManager, config::model::Config};

use super::agent_card::AgentCardService;
use super::config::A2aConfig;
use super::methods;
use super::storage::OxyTaskStorage;

/// Oxy implementation of the A2A handler trait.
///
/// This handler is scoped to a single agent and implements all A2A protocol
/// operations by delegating to Oxy's agent execution infrastructure.
///
/// # Fields
///
/// - `agent_name`: The name of the Oxy agent this handler executes
/// - `config`: Global Oxy configuration (includes A2A config)
/// - `storage`: Agent-scoped task storage for persistence
/// - `project_manager`: Project manager for accessing agent configs and execution context
/// - `agent_card_service`: Service for generating and caching agent cards
pub struct OxyA2aHandler {
    /// The name of the agent this handler is scoped to
    agent_name: String,
    /// Global Oxy configuration
    config: Arc<Config>,
    /// Agent-scoped task storage
    storage: Arc<OxyTaskStorage>,
    /// Database connection for auth and auditing
    db: Arc<DatabaseConnection>,
    /// Project manager for agent execution
    project_manager: Arc<ProjectManager>,
    /// Agent card service for generating agent cards
    agent_card_service: Arc<AgentCardService>,
    /// Base URL for constructing endpoint URLs in agent cards
    base_url: String,
}

impl OxyA2aHandler {
    /// Create a new handler instance scoped to a specific agent.
    ///
    /// # Arguments
    ///
    /// * `agent_name` - The name of the agent (from A2A configuration)
    /// * `config` - Global Oxy configuration
    /// * `storage` - Agent-scoped task storage
    /// * `db` - Database connection for auth and auditing
    /// * `project_manager` - Project manager for agent execution
    /// * `agent_card_service` - Service for generating agent cards
    /// * `base_url` - Base URL for constructing endpoint URLs (e.g., "https://api.example.com")
    ///
    /// # Returns
    ///
    /// A new handler instance that will execute the specified agent.
    pub fn new(
        agent_name: String,
        config: Arc<Config>,
        storage: Arc<OxyTaskStorage>,
        db: Arc<DatabaseConnection>,
        project_manager: Arc<ProjectManager>,
        agent_card_service: Arc<AgentCardService>,
        base_url: String,
    ) -> Self {
        Self {
            agent_name,
            config,
            storage,
            db,
            project_manager,
            agent_card_service,
            base_url,
        }
    }

    /// Get the A2A configuration from the global config.
    fn a2a_config(&self) -> Result<&A2aConfig, A2aError> {
        self.config
            .a2a
            .as_ref()
            .ok_or_else(|| A2aError::ServerError("A2A configuration not found".to_string()))
    }

    /// Get the agent configuration path from A2A config.
    fn agent_ref(&self) -> Result<String, A2aError> {
        let a2a_config = self.a2a_config()?;
        let agent_config = a2a_config.get_agent(&self.agent_name).ok_or_else(|| {
            A2aError::ServerError(format!(
                "Agent '{}' not found in A2A configuration",
                self.agent_name
            ))
        })?;
        Ok(agent_config.r#ref.clone())
    }

    async fn authenticate(&self, ctx: &A2aContext) -> Result<Option<A2aPrincipal>, A2aError> {
        // Check if Oxy authentication is enabled
        let oxy_config = crate::config::oxy::get_oxy_config().map_err(|e| {
            tracing::error!(
                target = "a2a::auth",
                agent = %self.agent_name,
                request_id = %ctx.request_id,
                "Failed to load Oxy configuration: {}",
                e
            );
            A2aError::ServerError("Failed to load authentication configuration".to_string())
        })?;

        let auth_enabled = oxy_config
            .authentication
            .as_ref()
            .map(|auth| auth.google.is_some() || auth.okta.is_some() || auth.basic.is_some())
            .unwrap_or(false);

        if !auth_enabled {
            tracing::info!(
                target = "a2a::auth",
                agent = %self.agent_name,
                request_id = %ctx.request_id,
                "A2A authentication not enabled, accepting request without authentication"
            );
            return Ok(None);
        }

        tracing::info!(
            target = "a2a::auth",
            agent = %self.agent_name,
            request_id = %ctx.request_id,
            "Authenticating A2A request using API key"
        );

        let (identity, validated_key) = authenticate_header_with_config(
            &self.db,
            &ctx.headers,
            DEFAULT_API_KEY_HEADER,
            &ApiKeyConfig::default(),
        )
        .await
        .map_err(|err| {
            tracing::warn!(
                target = "a2a::auth",
                agent = %self.agent_name,
                request_id = %ctx.request_id,
                "A2A authentication failed: {}",
                err
            );
            A2aError::Unauthorized(format!("API key authentication failed: {}", err))
        })?;

        let principal = A2aPrincipal {
            user_id: validated_key.user_id,
            api_key_id: validated_key.id,
            email: Some(identity.email),
            name: identity.name,
        };

        tracing::info!(
            target = "a2a::auth",
            agent = %self.agent_name,
            request_id = %ctx.request_id,
            user_id = %principal.user_id,
            api_key_id = %principal.api_key_id,
            "A2A authentication succeeded"
        );

        Ok(Some(principal))
    }

    fn principal_metadata(principal: &A2aPrincipal) -> HashMap<String, Value> {
        let principal_value = serde_json::json!({
            "user_id": principal.user_id,
            "api_key_id": principal.api_key_id,
            "name": principal.name,
            "email": principal.email,
            "auth_type": "api_key",
        });

        let mut map = HashMap::new();
        map.insert("principal".to_string(), principal_value);
        map
    }

    fn merge_metadata(target: &mut Option<HashMap<String, Value>>, extra: &HashMap<String, Value>) {
        let mut base = target.take().unwrap_or_default();
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
        if !base.is_empty() {
            *target = Some(base);
        }
    }

    fn attach_principal_metadata(
        target: &mut Option<HashMap<String, Value>>,
        principal: Option<&A2aPrincipal>,
    ) {
        if let Some(principal) = principal {
            let metadata = Self::principal_metadata(principal);
            Self::merge_metadata(target, &metadata);
        }
    }

    fn decorate_task_with_principal(task: &mut Task, principal: Option<&A2aPrincipal>) {
        if principal.is_none() {
            return;
        }

        let principal = principal.unwrap();
        let metadata = Self::principal_metadata(principal);
        Self::merge_metadata(&mut task.metadata, &metadata);

        if let Some(history) = task.history.as_mut() {
            for message in history.iter_mut() {
                Self::attach_principal_metadata(&mut message.metadata, Some(principal));
            }
        }

        if let Some(message) = task.status.message.as_mut() {
            Self::attach_principal_metadata(&mut message.metadata, Some(principal));
        }

        if let Some(artifacts) = task.artifacts.as_mut() {
            for artifact in artifacts.iter_mut() {
                Self::attach_principal_metadata(&mut artifact.metadata, Some(principal));
            }
        }
    }
}

#[async_trait]
impl A2aHandler for OxyA2aHandler {
    async fn authenticate_request(&self, ctx: &A2aContext) -> Result<(), A2aError> {
        self.authenticate(ctx).await?;
        Ok(())
    }

    /// Handle a synchronous message/send request.
    ///
    /// This method:
    /// 1. Validates the agent exists in A2A configuration
    /// 2. Loads the Oxy agent configuration
    /// 3. Converts the A2A message to Oxy format
    /// 4. Executes the agent
    /// 5. Stores the task in the database
    /// 6. Returns the task with artifacts
    async fn handle_send_message(
        &self,
        ctx: A2aContext,
        message: Message,
    ) -> Result<Task, A2aError> {
        let principal = self.authenticate(&ctx).await?;

        // Get agent ref for execution
        let agent_ref = self.agent_ref()?;

        // Delegate to message handler module
        let mut task = methods::message::handle_send_message(
            &self.agent_name,
            agent_ref,
            message,
            &self.project_manager,
        )
        .await?;

        Self::decorate_task_with_principal(&mut task, principal.as_ref());

        // Store task in database
        self.storage
            .create_task(task.clone())
            .await
            .map_err(|e| A2aError::ServerError(format!("Failed to store task: {}", e)))?;

        Ok(task)
    }

    /// Handle a streaming message/stream request.
    ///
    /// This method sets up an SSE stream for real-time agent execution updates.
    async fn handle_send_streaming_message(
        &self,
        ctx: A2aContext,
        message: Message,
    ) -> Result<SseStream, A2aError> {
        let principal = self.authenticate(&ctx).await?;
        let metadata = principal.as_ref().map(Self::principal_metadata);

        // Get agent ref for execution
        let agent_ref = self.agent_ref()?;

        // Delegate to streaming handler module
        methods::streaming::handle_send_streaming_message(
            &self.agent_name,
            agent_ref,
            message,
            &self.project_manager,
            self.storage.clone(),
            metadata,
        )
        .await
    }

    /// Handle agent card retrieval.
    ///
    /// This method generates an A2A agent card from the Oxy agent configuration
    /// using the AgentCardService for caching and generation.
    async fn handle_get_agent_card(&self, _ctx: A2aContext) -> Result<AgentCard, A2aError> {
        // Use the agent card service to generate/retrieve the cached card
        self.agent_card_service
            .get_agent_card(&self.agent_name, &self.base_url)
            .await
    }

    /// Get the task storage implementation.
    ///
    /// This is used by the router to handle task operations (get, cancel) directly.
    fn task_storage(&self) -> &dyn TaskStorage {
        self.storage.as_ref()
    }
}

#[derive(Debug, Clone)]
struct A2aPrincipal {
    pub user_id: Uuid,
    pub api_key_id: Uuid,
    pub email: Option<String>,
    pub name: Option<String>,
}
