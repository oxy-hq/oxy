//! Server abstractions for A2A protocol.
//!
//! This module provides server-side abstractions for implementing A2A protocol
//! handlers. It defines the core traits and types needed to build A2A-compliant
//! servers.
//!
//! # Architecture
//!
//! The server module is designed around a trait-based architecture:
//!
//! - **`A2aHandler` trait**: Defines the interface for implementing A2A method handlers
//! - **`A2aContext`**: Provides request context (metadata, headers, etc.)
//! - **Router builders**: Functions that create pre-configured axum routers
//!
//! # Single-Agent Focus
//!
//! This module operates on a single agent's perspective. Multi-agent orchestration
//! is handled by the consumer (e.g., core crate) by:
//! - Creating separate handler instances per agent
//! - Mounting routers at agent-specific paths
//! - Routing requests to the correct handler instance
//!
//! # Example
//!
//! ```rust,no_run
//! use a2a::server::{A2aHandler, A2aContext, SseStream};
//! use a2a::types::{Message, Task, AgentCard};
//! use a2a::storage::TaskStorage;
//! use a2a::error::A2aError;
//! use async_trait::async_trait;
//!
//! struct MyHandler {
//!     agent_name: String,
//!     // ... other fields
//! }
//!
//! #[async_trait]
//! impl A2aHandler for MyHandler {
//!     async fn handle_send_message(
//!         &self,
//!         ctx: A2aContext,
//!         message: Message,
//!     ) -> Result<Task, A2aError> {
//!         // Implementation
//!         todo!()
//!     }
//!
//!     async fn handle_send_streaming_message(
//!         &self,
//!         ctx: A2aContext,
//!         message: Message,
//!     ) -> Result<SseStream, A2aError> {
//!         // Implementation
//!         todo!()
//!     }
//!
//!     async fn handle_get_agent_card(
//!         &self,
//!         ctx: A2aContext,
//!     ) -> Result<AgentCard, A2aError> {
//!         // Implementation
//!         todo!()
//!     }
//!
//!     fn task_storage(&self) -> &dyn TaskStorage {
//!         // Return storage reference
//!         todo!()
//!     }
//! }
//! ```

use async_trait::async_trait;
use axum::response::IntoResponse;
use futures::Stream;
use http::{HeaderMap, StatusCode};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::A2aError;
use crate::http::HttpStatus;
use crate::storage::TaskStorage;
use crate::streaming::SseEvent;
use crate::types::{AgentCard, Message, Task};

const DEFAULT_AUTH_HEADER: &str = "X-API-Key";

/// Request context passed to all handler methods.
///
/// This struct contains contextual information about the request that may be
/// useful for processing, such as metadata, HTTP headers, and request IDs.
///
/// # Note on Agent Identity
///
/// This context does NOT contain an `agent_name` field. Handler instances are
/// scoped to specific agents, with the agent name as a field on the handler
/// struct itself. This design ensures clear separation of concerns and makes
/// it impossible to accidentally route a request to the wrong agent.
#[derive(Debug, Clone)]
pub struct A2aContext {
    /// Optional request metadata.
    ///
    /// This can contain arbitrary key-value pairs provided by the client
    /// or added by middleware (e.g., authentication info, correlation IDs).
    pub metadata: HashMap<String, Value>,

    /// HTTP headers from the request.
    ///
    /// Useful for extracting authentication tokens, content types,
    /// user agents, etc. This uses axum's `HeaderMap` type for compatibility
    /// with HTTP server frameworks.
    pub headers: HeaderMap,

    /// Request ID for tracing and logging.
    ///
    /// This should be unique per request and can be used to correlate
    /// logs and events related to the same request.
    pub request_id: String,
}

impl A2aContext {
    /// Create a new context with the given request ID.
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            metadata: HashMap::new(),
            headers: HeaderMap::new(),
            request_id: request_id.into(),
        }
    }

    /// Add metadata to the context.
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Add a header to the context.
    ///
    /// # Panics
    ///
    /// Panics if the header name or value are invalid.
    pub fn with_header(mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        use http::header::HeaderName;
        use http::HeaderValue;

        let name = HeaderName::from_bytes(key.as_ref().as_bytes()).expect("invalid header name");
        let val = HeaderValue::from_str(value.as_ref()).expect("invalid header value");
        self.headers.insert(name, val);
        self
    }

    /// Get metadata by key.
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }

    /// Get header by key.
    ///
    /// Returns the header value as a string slice if present and valid UTF-8.
    pub fn get_header(&self, key: &str) -> Option<&str> {
        use http::header::HeaderName;

        let name = HeaderName::from_bytes(key.as_bytes()).ok()?;
        self.headers.get(&name)?.to_str().ok()
    }
}

impl Default for A2aContext {
    fn default() -> Self {
        Self::new(uuid::Uuid::new_v4().to_string())
    }
}

fn build_context_from_headers(headers: HeaderMap) -> A2aContext {
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut ctx = A2aContext::new(request_id);
    ctx.headers = headers;
    ctx
}

fn add_www_authenticate_header_for_status(
    response: &mut axum::response::Response,
    status: HttpStatus,
) {
    if matches!(status, HttpStatus::Unauthorized | HttpStatus::Forbidden) {
        if let Ok(value) = http::HeaderValue::from_str(&format!(
            "ApiKey realm=\"a2a\", header=\"{}\"",
            DEFAULT_AUTH_HEADER
        )) {
            response
                .headers_mut()
                .insert(http::header::WWW_AUTHENTICATE, value);
        }
    }
}

/// Type alias for SSE event streams.
///
/// This represents an asynchronous stream of Server-Sent Events that can be
/// returned from streaming handlers.
pub type SseStream = Pin<Box<dyn Stream<Item = Result<SseEvent, A2aError>> + Send>>;

/// Handler trait for A2A protocol operations.
///
/// Implementations of this trait provide the business logic for handling A2A
/// protocol requests. The trait is designed to be implemented by consumer
/// applications (e.g., Oxy core) with their specific logic.
///
/// # Handler Scoping
///
/// Each handler instance should be scoped to a single agent. The agent name
/// should be a field on the implementing struct, not passed in the context.
/// This ensures clear separation and prevents routing errors.
///
/// # Method Overview
///
/// - **`handle_send_message`**: Process a message and return a task
/// - **`handle_send_streaming_message`**: Process a message with streaming updates
/// - **`handle_get_agent_card`**: Return the agent's capability card
/// - **`task_storage`**: Provide access to task storage (used by router for task operations)
///
/// # Task Operations
///
/// Note that task retrieval and cancellation operations (tasks/get, tasks/cancel)
/// are handled automatically by the router using the `TaskStorage` trait.
/// Handlers only need to implement message processing and agent card retrieval.
#[async_trait]
pub trait A2aHandler: Send + Sync {
    /// Authenticate the incoming request if needed.
    ///
    /// Default implementation allows anonymous access. Implementers can override
    /// to enforce authentication for all routes.
    async fn authenticate_request(&self, _ctx: &A2aContext) -> Result<(), A2aError> {
        Ok(())
    }

    /// Handle a message/send request.
    ///
    /// This method processes an incoming message and returns a task representing
    /// the processing work. The task may be in any state (working, completed, etc.)
    /// depending on whether processing is synchronous or asynchronous.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Request context with metadata and headers
    /// * `message` - The message to process
    ///
    /// # Returns
    ///
    /// A `Task` representing the work initiated by this message. The task should
    /// include:
    /// - A unique ID
    /// - Current status (state and optional message)
    /// - Any artifacts generated so far (may be empty if still working)
    /// - Optional context ID for grouping related tasks
    ///
    /// # Errors
    ///
    /// Returns `A2aError` if the message cannot be processed. Common error cases:
    /// - Invalid message format
    /// - Agent not found or not configured
    /// - Internal processing errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use a2a::server::{A2aHandler, A2aContext, SseStream};
    /// # use a2a::types::{Message, Task, TaskStatus, TaskState};
    /// # use a2a::error::A2aError;
    /// # use async_trait::async_trait;
    /// # struct MyHandler;
    /// # #[async_trait]
    /// # impl A2aHandler for MyHandler {
    /// async fn handle_send_message(
    ///     &self,
    ///     ctx: A2aContext,
    ///     message: Message,
    /// ) -> Result<Task, A2aError> {
    ///     // Process the message
    ///     let task_id = uuid::Uuid::new_v4().to_string();
    ///     let context_id = uuid::Uuid::new_v4().to_string();
    ///     
    ///     // Create a task representing the work
    ///     let task = Task::new(
    ///         context_id,
    ///         TaskStatus::new(TaskState::Working),
    ///     );
    ///     
    ///     Ok(task)
    /// }
    /// # async fn handle_send_streaming_message(&self, ctx: A2aContext, message: Message) -> Result<SseStream, A2aError> { todo!() }
    /// # async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<a2a::types::AgentCard, A2aError> { todo!() }
    /// # fn task_storage(&self) -> &dyn a2a::storage::TaskStorage { todo!() }
    /// # }
    /// ```
    async fn handle_send_message(
        &self,
        ctx: A2aContext,
        message: Message,
    ) -> Result<Task, A2aError>;

    /// Handle a message/stream request with Server-Sent Events.
    ///
    /// This method processes an incoming message and returns a stream of events
    /// representing the progress of processing. This allows clients to receive
    /// real-time updates as the agent works on the task.
    ///
    /// # Event Stream
    ///
    /// The stream should emit events in this order:
    /// 1. `task.created` - Initial task creation
    /// 2. `task.progress` - Progress updates during execution (optional, multiple)
    /// 3. `artifact.created` - As artifacts are generated (optional, multiple)
    /// 4. `task.completed` - Final completion with all artifacts
    ///    OR `task.failed` - If execution fails
    ///
    /// # Arguments
    ///
    /// * `ctx` - Request context with metadata and headers
    /// * `message` - The message to process
    ///
    /// # Returns
    ///
    /// A stream of `SseEvent` items that represent the processing progress.
    /// Each event contains data that can be serialized to JSON.
    ///
    /// # Errors
    ///
    /// Returns `A2aError` if streaming cannot be initiated. Errors during
    /// streaming should be sent as SSE error events when possible.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use a2a::server::{A2aHandler, A2aContext, SseStream};
    /// # use a2a::types::Message;
    /// # use a2a::error::A2aError;
    /// # use a2a::streaming::{SseEvent, SseEventType};
    /// # use async_trait::async_trait;
    /// # use futures::stream;
    /// # struct MyHandler;
    /// # #[async_trait]
    /// # impl A2aHandler for MyHandler {
    /// async fn handle_send_streaming_message(
    ///     &self,
    ///     ctx: A2aContext,
    ///     message: Message,
    /// ) -> Result<SseStream, A2aError> {
    ///     // Create an event stream
    ///     let events = vec![
    ///         SseEvent::with_type(
    ///             SseEventType::TaskCreated,
    ///             r#"{"task": {"id": "task-1", "status": {"state": "working"}}}"#,
    ///         ),
    ///         SseEvent::with_type(
    ///             SseEventType::TaskCompleted,
    ///             r#"{"task": {"id": "task-1", "status": {"state": "completed"}}}"#,
    ///         ),
    ///     ];
    ///     
    ///     let stream = stream::iter(events.into_iter().map(Ok));
    ///     Ok(Box::pin(stream))
    /// }
    /// # async fn handle_send_message(&self, ctx: A2aContext, message: Message) -> Result<a2a::types::Task, A2aError> { todo!() }
    /// # async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<a2a::types::AgentCard, A2aError> { todo!() }
    /// # fn task_storage(&self) -> &dyn a2a::storage::TaskStorage { todo!() }
    /// # }
    /// ```
    async fn handle_send_streaming_message(
        &self,
        ctx: A2aContext,
        message: Message,
    ) -> Result<SseStream, A2aError>;

    /// Handle an agent/getAuthenticatedExtendedCard request.
    ///
    /// This method returns the agent's capability card, which describes:
    /// - Agent name and description
    /// - Available skills and their capabilities
    /// - Supported transports and endpoints
    /// - Authentication requirements
    ///
    /// # Arguments
    ///
    /// * `ctx` - Request context with metadata and headers
    ///
    /// # Returns
    ///
    /// An `AgentCard` describing this agent's capabilities.
    ///
    /// # Errors
    ///
    /// Returns `A2aError` if the agent card cannot be generated.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use a2a::server::{A2aHandler, A2aContext, SseStream};
    /// # use a2a::types::{AgentCard, AgentSkill, TransportProtocol, AgentInterface};
    /// # use a2a::error::A2aError;
    /// # use async_trait::async_trait;
    /// # struct MyHandler { agent_name: String }
    /// # #[async_trait]
    /// # impl A2aHandler for MyHandler {
    /// async fn handle_get_agent_card(
    ///     &self,
    ///     ctx: A2aContext,
    /// ) -> Result<AgentCard, A2aError> {
    ///     let mut card = AgentCard::new(
    ///         self.agent_name.clone(),
    ///         "My helpful agent",
    ///         format!("https://example.com/agents/{}/v1/jsonrpc", self.agent_name),
    ///     );
    ///     
    ///     // Add additional HTTP interface
    ///     card.additional_interfaces = Some(vec![
    ///         AgentInterface {
    ///             url: format!("https://example.com/agents/{}/v1", self.agent_name),
    ///             transport: TransportProtocol::HttpJson,
    ///         }
    ///     ]);
    ///     
    ///     Ok(card)
    /// }
    /// # async fn handle_send_message(&self, ctx: A2aContext, message: a2a::types::Message) -> Result<a2a::types::Task, A2aError> { todo!() }
    /// # async fn handle_send_streaming_message(&self, ctx: A2aContext, message: a2a::types::Message) -> Result<SseStream, A2aError> { todo!() }
    /// # fn task_storage(&self) -> &dyn a2a::storage::TaskStorage { todo!() }
    /// # }
    /// ```
    async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<AgentCard, A2aError>;

    /// Get the task storage implementation.
    ///
    /// This method provides access to the storage layer for task operations.
    /// The router uses this to handle task retrieval and cancellation requests
    /// automatically, without requiring additional handler methods.
    ///
    /// # Returns
    ///
    /// A reference to the task storage implementation. The storage should be
    /// scoped to the same agent as the handler.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use a2a::server::{A2aHandler, A2aContext, SseStream};
    /// # use a2a::types::{Message, Task, AgentCard};
    /// # use a2a::storage::TaskStorage;
    /// # use a2a::error::A2aError;
    /// # use async_trait::async_trait;
    /// # use std::sync::Arc;
    /// # struct MyHandler { storage: Arc<dyn TaskStorage> }
    /// # #[async_trait]
    /// # impl A2aHandler for MyHandler {
    /// fn task_storage(&self) -> &dyn TaskStorage {
    ///     self.storage.as_ref()
    /// }
    /// # async fn handle_send_message(&self, ctx: A2aContext, message: Message) -> Result<Task, A2aError> { todo!() }
    /// # async fn handle_send_streaming_message(&self, ctx: A2aContext, message: Message) -> Result<SseStream, A2aError> { todo!() }
    /// # async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<AgentCard, A2aError> { todo!() }
    /// # }
    /// ```
    fn task_storage(&self) -> &dyn TaskStorage;
}

/// Create an axum Router with JSON-RPC endpoint for a single agent.
///
/// This function returns a pre-configured axum router that implements the A2A
/// JSON-RPC 2.0 protocol. The router handles the following:
///
/// - POST `/jsonrpc` - Main JSON-RPC endpoint that accepts all A2A methods
///
/// # Supported Methods
///
/// - `message/send` - Send a message and get a task response
/// - `agent/getAuthenticatedExtendedCard` - Get agent capabilities
/// - `tasks/get` - Retrieve task status (handled via storage)
/// - `tasks/cancel` - Cancel a running task (handled via storage)
///
/// # Handler Scoping
///
/// The handler instance should be scoped to a single agent. Multi-agent routing
/// is the responsibility of the consumer (e.g., core crate) which should:
/// - Create separate handler instances per agent
/// - Mount returned routers at agent-specific paths
///
/// # Request Flow
///
/// 1. Parse JSON-RPC request body
/// 2. Validate JSON-RPC 2.0 format
/// 3. Extract method name and parameters
/// 4. Build A2aContext from request metadata
/// 5. Dispatch to appropriate handler method
/// 6. Convert result to JSON-RPC response
/// 7. Handle errors and return JSON-RPC error responses
///
/// # Example
///
/// ```rust,no_run
/// use a2a::server::{create_jsonrpc_router, A2aHandler};
/// use std::sync::Arc;
/// # use a2a::server::{A2aContext, SseStream};
/// # use a2a::types::{Message, Task, AgentCard};
/// # use a2a::storage::TaskStorage;
/// # use a2a::error::A2aError;
/// # use async_trait::async_trait;
///
/// # struct MyHandler;
/// # #[async_trait]
/// # impl A2aHandler for MyHandler {
/// #     async fn handle_send_message(&self, ctx: A2aContext, message: Message) -> Result<Task, A2aError> { todo!() }
/// #     async fn handle_send_streaming_message(&self, ctx: A2aContext, message: Message) -> Result<SseStream, A2aError> { todo!() }
/// #     async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<AgentCard, A2aError> { todo!() }
/// #     fn task_storage(&self) -> &dyn TaskStorage { todo!() }
/// # }
///
/// // Create handler instance for a specific agent
/// let handler = Arc::new(MyHandler);
///
/// // Get router from a2a crate
/// let router = create_jsonrpc_router(handler);
///
/// // Mount at agent-specific path (consumer's responsibility)
/// // app.nest("/a2a/agents/my-agent/v1", router);
/// ```
///
/// # Errors
///
/// The router returns JSON-RPC error responses for:
/// - Parse errors (invalid JSON)
/// - Invalid JSON-RPC format
/// - Unknown methods
/// - Handler errors
/// - Internal server errors
pub fn create_jsonrpc_router<H>(handler: Arc<H>) -> axum::Router
where
    H: A2aHandler + 'static,
{
    use axum::routing::post;

    axum::Router::new()
        .route("/jsonrpc", post(jsonrpc_handler::<H>))
        .with_state(handler)
}

/// Internal handler for JSON-RPC requests.
///
/// This function processes incoming JSON-RPC requests and dispatches them
/// to the appropriate handler methods based on the method name.
///
/// Note: This handler returns JSON responses. For streaming methods like
/// `message/stream`, a separate SSE-capable handler is needed.
async fn jsonrpc_handler<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
    axum::extract::Json(request): axum::extract::Json<crate::jsonrpc::JsonRpcRequest>,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::error_to_http_status;
    use crate::http::HttpStatus;
    use crate::jsonrpc::JsonRpcResponse;
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::stream::StreamExt;

    // Validate JSON-RPC version
    if let Err(e) = request.validate_version() {
        let error = crate::error::JsonRpcError::invalid_request(e);
        let id = request.id.clone().unwrap_or(serde_json::Value::Null);
        let response = JsonRpcResponse::error(error, id);
        let mut resp = axum::Json(response).into_response();
        *resp.status_mut() = StatusCode::BAD_REQUEST;
        return resp;
    }

    let id = request.id.clone().unwrap_or(serde_json::Value::Null);

    // Build context from request
    let ctx = build_context_from_headers(headers.clone());
    // TODO: Extract metadata from request.params if present

    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error = crate::error::JsonRpcError::from_a2a_error(e);
        let response = JsonRpcResponse::error(error, id.clone());
        let mut resp = axum::Json(response).into_response();
        *resp.status_mut() =
            StatusCode::from_u16(status.code()).unwrap_or(StatusCode::UNAUTHORIZED);
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Handle message/stream separately since it returns SSE
    if request.method == "message/stream" {
        let params: crate::jsonrpc::MessageSendParams = match request.params {
            Some(p) => match serde_json::from_value(p) {
                Ok(params) => params,
                Err(e) => {
                    let error = crate::error::JsonRpcError::invalid_params(format!(
                        "Invalid message/stream params: {}",
                        e
                    ));
                    let response = JsonRpcResponse::error(error, id);
                    let mut resp = axum::Json(response).into_response();
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return resp;
                }
            },
            None => {
                let error =
                    crate::error::JsonRpcError::invalid_params("message/stream requires params");
                let response = JsonRpcResponse::error(error, id);
                let mut resp = axum::Json(response).into_response();
                *resp.status_mut() = StatusCode::BAD_REQUEST;
                return resp;
            }
        };

        // Get the SSE stream from the handler
        let stream = match handler
            .handle_send_streaming_message(ctx, params.message)
            .await
        {
            Ok(stream) => stream,
            Err(e) => {
                let status = error_to_http_status(&e);
                let error = crate::error::JsonRpcError::from_a2a_error(e);
                let response = JsonRpcResponse::error(error, id.clone());
                let mut resp = axum::Json(response).into_response();
                *resp.status_mut() = StatusCode::from_u16(status.code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                add_www_authenticate_header_for_status(&mut resp, status);
                return resp;
            }
        };

        // Convert SSE stream to axum SSE response
        // Each event data contains a complete JSON-RPC response
        let id_clone = id.clone();
        let sse_stream = stream.map(move |result| {
            match result {
                Ok(sse_event) => {
                    // Parse the SSE event data as JSON value
                    let data_value: serde_json::Value = match serde_json::from_str(&sse_event.data)
                    {
                        Ok(v) => v,
                        Err(_e) => {
                            // If data is not valid JSON, wrap it as a string
                            serde_json::Value::String(sse_event.data.clone())
                        }
                    };

                    // Wrap the data in a JSON-RPC response
                    let json_rpc_response = JsonRpcResponse::success(data_value, id_clone.clone());
                    match serde_json::to_string(&json_rpc_response) {
                        Ok(data) => {
                            let mut event = Event::default().data(data);
                            // Preserve event type if present
                            if let Some(event_type) = &sse_event.event_type {
                                event = event.event(event_type.to_string());
                            }
                            // Preserve event ID if present
                            if let Some(event_id) = &sse_event.id {
                                event = event.id(event_id.clone());
                            }
                            Ok::<_, std::convert::Infallible>(event)
                        }
                        Err(e) => {
                            let error = crate::error::JsonRpcError::internal_error(format!(
                                "Failed to serialize response: {}",
                                e
                            ));
                            let response = JsonRpcResponse::error(error, id_clone.clone());
                            let data = serde_json::to_string(&response).unwrap_or_default();
                            Ok::<_, std::convert::Infallible>(Event::default().data(data))
                        }
                    }
                }
                Err(e) => {
                    let error = crate::error::JsonRpcError::from_a2a_error(e);
                    let response = JsonRpcResponse::error(error, id_clone.clone());
                    let data = serde_json::to_string(&response).unwrap_or_default();
                    Ok::<_, std::convert::Infallible>(Event::default().data(data))
                }
            }
        });

        return Sse::new(sse_stream)
            .keep_alive(KeepAlive::default())
            .into_response();
    }

    // Dispatch other methods
    let result = match request.method.as_str() {
        "message/send" => handle_message_send(handler.as_ref(), ctx, request.params).await,
        "agent/getAuthenticatedExtendedCard" => handle_get_agent_card(handler.as_ref(), ctx).await,
        "tasks/get" => handle_tasks_get(handler.as_ref(), ctx, request.params).await,
        "tasks/cancel" => handle_tasks_cancel(handler.as_ref(), ctx, request.params).await,
        "tasks/resubscribe" => Err(A2aError::UnsupportedOperation(
            "tasks/resubscribe is not yet implemented".to_string(),
        )),
        _ => Err(A2aError::MethodNotFound(request.method.clone())),
    };

    // Convert result to JSON-RPC response
    let (response, status) = match result {
        Ok(value) => (JsonRpcResponse::success(value, id), HttpStatus::Ok),
        Err(e) => {
            let status = error_to_http_status(&e);
            let error = crate::error::JsonRpcError::from_a2a_error(e);
            (JsonRpcResponse::error(error, id), status)
        }
    };

    let mut resp = axum::Json(response).into_response();
    *resp.status_mut() = StatusCode::from_u16(status.code()).unwrap_or(StatusCode::OK);
    add_www_authenticate_header_for_status(&mut resp, status);
    resp
}

/// Handle message/send method
async fn handle_message_send<H>(
    handler: &H,
    ctx: A2aContext,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, A2aError>
where
    H: A2aHandler,
{
    let params: crate::jsonrpc::MessageSendParams = match params {
        Some(p) => serde_json::from_value(p)
            .map_err(|e| A2aError::InvalidParams(format!("Invalid message/send params: {}", e)))?,
        None => {
            return Err(A2aError::InvalidParams(
                "message/send requires params".to_string(),
            ))
        }
    };

    let task = handler.handle_send_message(ctx, params.message).await?;

    serde_json::to_value(&task)
        .map_err(|e| A2aError::SerializationError(format!("Failed to serialize task: {}", e)))
}

/// Handle agent/getAuthenticatedExtendedCard method
async fn handle_get_agent_card<H>(
    handler: &H,
    ctx: A2aContext,
) -> Result<serde_json::Value, A2aError>
where
    H: A2aHandler,
{
    let card = handler.handle_get_agent_card(ctx).await?;

    serde_json::to_value(&card)
        .map_err(|e| A2aError::SerializationError(format!("Failed to serialize agent card: {}", e)))
}

/// Handle tasks/get method
async fn handle_tasks_get<H>(
    handler: &H,
    _ctx: A2aContext,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, A2aError>
where
    H: A2aHandler,
{
    let params: crate::jsonrpc::TaskQueryParams = match params {
        Some(p) => serde_json::from_value(p)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid tasks/get params: {}", e)))?,
        None => {
            return Err(A2aError::InvalidTask(
                "tasks/get requires params".to_string(),
            ))
        }
    };

    let task = handler
        .task_storage()
        .get_task(params.id.clone())
        .await?
        .ok_or_else(|| A2aError::TaskNotFound(params.id.clone()))?;

    serde_json::to_value(&task)
        .map_err(|e| A2aError::InternalError(format!("Failed to serialize task: {}", e)))
}

/// Handle tasks/cancel method
async fn handle_tasks_cancel<H>(
    handler: &H,
    _ctx: A2aContext,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, A2aError>
where
    H: A2aHandler,
{
    let params: crate::jsonrpc::TaskIdParams = match params {
        Some(p) => serde_json::from_value(p)
            .map_err(|e| A2aError::InvalidTask(format!("Invalid tasks/cancel params: {}", e)))?,
        None => {
            return Err(A2aError::InvalidTask(
                "tasks/cancel requires params".to_string(),
            ))
        }
    };

    // Get the task
    let mut task = handler
        .task_storage()
        .get_task(params.id.clone())
        .await?
        .ok_or_else(|| A2aError::TaskNotFound(params.id.clone()))?;

    // Update state to cancelled
    task.status.state = crate::types::TaskState::Canceled;
    task.status.message = Some(crate::types::Message::new_agent(vec![
        crate::types::Part::Text(crate::types::TextPart::new("Task cancelled by user")),
    ]));

    // Store updated task
    let updated_task = handler.task_storage().update_task(task).await?;

    serde_json::to_value(&updated_task)
        .map_err(|e| A2aError::SerializationError(format!("Failed to serialize task: {}", e)))
}

/// Create an axum Router with HTTP+JSON endpoints for a single agent.
///
/// This function returns a pre-configured axum router that implements the A2A
/// HTTP+JSON/REST protocol. The router handles the following endpoints:
///
/// - POST `/messages` - Send a message and get a task response
/// - POST `/messages/stream` - Send a message and receive SSE stream
/// - GET `/tasks/{id}` - Retrieve task status
/// - DELETE `/tasks/{id}` - Delete/cancel a task
/// - GET `/agent` - Get agent capabilities
///
/// # Handler Scoping
///
/// The handler instance should be scoped to a single agent. Multi-agent routing
/// is the responsibility of the consumer (e.g., core crate) which should:
/// - Create separate handler instances per agent
/// - Mount returned routers at agent-specific paths
///
/// # Request Flow
///
/// 1. Parse HTTP request (method, path, body)
/// 2. Extract path parameters (e.g., task ID)
/// 3. Validate request body format
/// 4. Build A2aContext from request metadata
/// 5. Dispatch to appropriate handler method or storage
/// 6. Convert result to HTTP response
/// 7. Handle errors and return appropriate HTTP status codes
///
/// # Example
///
/// ```rust,no_run
/// use a2a::server::{create_http_router, A2aHandler};
/// use std::sync::Arc;
/// # use a2a::server::{A2aContext, SseStream};
/// # use a2a::types::{Message, Task, AgentCard};
/// # use a2a::storage::TaskStorage;
/// # use a2a::error::A2aError;
/// # use async_trait::async_trait;
///
/// # struct MyHandler;
/// # #[async_trait]
/// # impl A2aHandler for MyHandler {
/// #     async fn handle_send_message(&self, ctx: A2aContext, message: Message) -> Result<Task, A2aError> { todo!() }
/// #     async fn handle_send_streaming_message(&self, ctx: A2aContext, message: Message) -> Result<SseStream, A2aError> { todo!() }
/// #     async fn handle_get_agent_card(&self, ctx: A2aContext) -> Result<AgentCard, A2aError> { todo!() }
/// #     fn task_storage(&self) -> &dyn TaskStorage { todo!() }
/// # }
///
/// // Create handler instance for a specific agent
/// let handler = Arc::new(MyHandler);
///
/// // Get router from a2a crate
/// let router = create_http_router(handler);
///
/// // Mount at agent-specific path (consumer's responsibility)
/// // app.nest("/a2a/agents/my-agent/v1", router);
/// ```
///
/// # Errors
///
/// The router returns HTTP error responses with appropriate status codes:
/// - 400 Bad Request - Invalid request format or parameters
/// - 404 Not Found - Task not found or unknown endpoint
/// - 500 Internal Server Error - Handler errors or internal failures
pub fn create_http_router<H>(handler: Arc<H>) -> axum::Router
where
    H: A2aHandler + 'static,
{
    use axum::routing::{delete, get, post};

    axum::Router::new()
        .route("/messages", post(http_send_message::<H>))
        .route("/messages/stream", post(http_send_streaming_message::<H>))
        .route("/tasks/{id}", get(http_get_task::<H>))
        .route("/tasks/{id}", delete(http_delete_task::<H>))
        .route("/agent", get(http_get_agent_card::<H>))
        .with_state(handler)
}

/// HTTP handler for POST /messages
async fn http_send_message<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
    axum::extract::Json(request): axum::extract::Json<crate::http::SendMessageRequest>,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::{error_to_http_status, HttpErrorResponse, SendMessageResponse};

    // Build context from request
    let ctx = build_context_from_headers(headers);

    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error_response = HttpErrorResponse::from(e);
        let mut resp = (
            axum::http::StatusCode::from_u16(status.code()).unwrap(),
            axum::Json(error_response),
        )
            .into_response();
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Call handler
    let result = handler.handle_send_message(ctx, request.message).await;

    match result {
        Ok(task) => {
            let response = SendMessageResponse { task };
            axum::Json(response).into_response()
        }
        Err(e) => {
            let status = error_to_http_status(&e);
            let error_response = HttpErrorResponse::from(e);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            resp
        }
    }
}

/// HTTP handler for POST /messages/stream
async fn http_send_streaming_message<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
    axum::extract::Json(request): axum::extract::Json<crate::http::StreamMessageRequest>,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::{error_to_http_status, HttpErrorResponse};
    use axum::response::sse::{Event, KeepAlive, Sse};
    use futures::stream::StreamExt;

    // Build context from request
    let ctx = build_context_from_headers(headers);

    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error_response = HttpErrorResponse::from(e);
        let mut resp = (
            axum::http::StatusCode::from_u16(status.code()).unwrap(),
            axum::Json(error_response),
        )
            .into_response();
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Get the SSE stream from the handler
    let stream = match handler
        .handle_send_streaming_message(ctx, request.message)
        .await
    {
        Ok(stream) => stream,
        Err(e) => {
            let status = error_to_http_status(&e);
            let error_response = HttpErrorResponse::from(e);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            return resp;
        }
    };

    // Convert SSE stream to axum SSE response
    let sse_stream = stream.map(|result| match result {
        Ok(sse_event) => {
            let mut event = Event::default().data(sse_event.data);
            // Preserve event type if present
            if let Some(event_type) = &sse_event.event_type {
                event = event.event(event_type.to_string());
            }
            // Preserve event ID if present
            if let Some(event_id) = &sse_event.id {
                event = event.id(event_id.clone());
            }
            Ok::<_, std::convert::Infallible>(event)
        }
        Err(e) => {
            // Send error as SSE event
            let error_response = HttpErrorResponse::from(e);
            let data = serde_json::to_string(&error_response).unwrap_or_default();
            Ok::<_, std::convert::Infallible>(Event::default().event("error").data(data))
        }
    });

    Sse::new(sse_stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// HTTP handler for GET /tasks/:id
async fn http_get_task<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::{error_to_http_status, GetTaskResponse, HttpErrorResponse};

    let ctx = build_context_from_headers(headers);
    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error_response = HttpErrorResponse::from(e);
        let mut resp = (
            axum::http::StatusCode::from_u16(status.code()).unwrap(),
            axum::Json(error_response),
        )
            .into_response();
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Query storage directly
    let result = handler.task_storage().get_task(task_id.clone()).await;

    match result {
        Ok(Some(task)) => {
            let response = GetTaskResponse { task };
            axum::Json(response).into_response()
        }
        Ok(None) => {
            let error = A2aError::TaskNotFound(task_id);
            let status = error_to_http_status(&error);
            let error_response = HttpErrorResponse::from(error);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            resp
        }
        Err(e) => {
            let status = error_to_http_status(&e);
            let error_response = HttpErrorResponse::from(e);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            resp
        }
    }
}

/// HTTP handler for DELETE /tasks/:id
async fn http_delete_task<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
    axum::extract::Path(task_id): axum::extract::Path<String>,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::{error_to_http_status, HttpErrorResponse};

    let ctx = build_context_from_headers(headers);
    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error_response = HttpErrorResponse::from(e);
        let mut resp = (
            axum::http::StatusCode::from_u16(status.code()).unwrap(),
            axum::Json(error_response),
        )
            .into_response();
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Delete task via storage
    let result = handler.task_storage().delete_task(task_id.clone()).await;

    match result {
        Ok(()) => {
            // Return 204 No Content on success
            axum::http::StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            let status = error_to_http_status(&e);
            let error_response = HttpErrorResponse::from(e);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            resp
        }
    }
}

/// HTTP handler for GET /agent
async fn http_get_agent_card<H>(
    axum::extract::State(handler): axum::extract::State<Arc<H>>,
    headers: HeaderMap,
) -> axum::response::Response
where
    H: A2aHandler,
{
    use crate::http::{error_to_http_status, GetAgentCardResponse, HttpErrorResponse};

    // Build context from request
    let ctx = build_context_from_headers(headers);

    if let Err(e) = handler.authenticate_request(&ctx).await {
        let status = error_to_http_status(&e);
        let error_response = HttpErrorResponse::from(e);
        let mut resp = (
            axum::http::StatusCode::from_u16(status.code()).unwrap(),
            axum::Json(error_response),
        )
            .into_response();
        add_www_authenticate_header_for_status(&mut resp, status);
        return resp;
    }

    // Call handler
    let result = handler.handle_get_agent_card(ctx).await;

    match result {
        Ok(card) => {
            let response = GetAgentCardResponse { card };
            axum::Json(response).into_response()
        }
        Err(e) => {
            let status = error_to_http_status(&e);
            let error_response = HttpErrorResponse::from(e);
            let mut resp = (
                axum::http::StatusCode::from_u16(status.code()).unwrap(),
                axum::Json(error_response),
            )
                .into_response();
            add_www_authenticate_header_for_status(&mut resp, status);
            resp
        }
    }
}
