//! Core A2A protocol data structures
//!
//! This module contains all the data types defined in the A2A protocol specification,
//! including Messages, Tasks, Parts, AgentCards, and related structures.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Transport protocol types supported by A2A
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum TransportProtocol {
    /// JSON-RPC 2.0 over HTTP
    #[serde(rename = "JSONRPC")]
    JsonRpc,
    /// gRPC over HTTP/2
    #[serde(rename = "GRPC")]
    Grpc,
    /// REST-style HTTP with JSON
    #[serde(rename = "HTTP+JSON")]
    HttpJson,
}

/// Lifecycle states of a Task
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    /// The task has been submitted and is awaiting execution
    Submitted,
    /// The agent is actively working on the task
    Working,
    /// The task is paused and waiting for input from the user
    InputRequired,
    /// The task has been successfully completed
    Completed,
    /// The task has been canceled by the user
    Canceled,
    /// The task failed due to an error during execution
    Failed,
    /// The task was rejected by the agent and was not started
    Rejected,
    /// The task requires authentication to proceed
    AuthRequired,
    /// The task is in an unknown or indeterminate state
    Unknown,
}

/// Role in a message exchange
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user/client
    User,
    /// Message from the agent
    Agent,
}

/// Message kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum MessageKind {
    /// Message
    #[default]
    Message,
}

/// Base properties common to all message or artifact parts
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PartMetadata {
    /// Additional metadata for the part
    #[serde(flatten, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A text segment within a message or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextPart {
    /// Kind of text content
    #[serde(rename = "kind", default)]
    pub kind: TextKind,
    /// The string content of the text part
    pub text: String,
    /// Additional metadata for the text part
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Text kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TextKind {
    /// Plain text
    #[default]
    Text,
}

impl TextPart {
    /// Create a new text part
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            kind: TextKind::Text,
            text: text.into(),
            metadata: None,
        }
    }
}

/// Base properties for a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileBase {
    /// An optional name for the file (e.g., "document.pdf")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The MIME type of the file (e.g., "application/pdf")
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// File with content provided directly as base64-encoded bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWithBytes {
    /// Base file properties
    #[serde(flatten)]
    pub base: FileBase,
    /// The base64-encoded content of the file
    pub bytes: String,
}

/// File with content located at a specific URI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileWithUri {
    /// Base file properties
    #[serde(flatten)]
    pub base: FileBase,
    /// A URL pointing to the file's content
    pub uri: String,
}

/// File content, either as bytes or URI
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FileContent {
    /// File with inline bytes
    Bytes(FileWithBytes),
    /// File with URI reference
    Uri(FileWithUri),
}

/// A file segment within a message or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePart {
    /// Kind of file content
    #[serde(rename = "kind", default)]
    pub kind: FileKind,
    /// The file content, as either bytes or URI
    #[serde(flatten)]
    pub file: FileContent,
    /// Additional metadata for the file
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// File kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum FileKind {
    /// File content
    #[default]
    File,
}

/// A structured data segment (e.g., JSON) within a message or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPart {
    /// Kind of data content
    #[serde(rename = "kind", default)]
    pub kind: DataKind,
    /// The structured data content
    pub data: serde_json::Value,
    /// Additional metadata for the data
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Data kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DataKind {
    /// Structured data
    #[default]
    Data,
}

impl DataPart {
    /// Create a new data part
    pub fn new(data: serde_json::Value) -> Self {
        Self {
            kind: DataKind::Data,
            data,
            metadata: None,
        }
    }
}

/// A discriminated union representing a part of a message or artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Part {
    /// Text content part
    Text(TextPart),
    /// File content part
    File(FilePart),
    /// Structured data part
    Data(DataPart),
}

impl From<TextPart> for Part {
    fn from(part: TextPart) -> Self {
        Part::Text(part)
    }
}

impl From<FilePart> for Part {
    fn from(part: FilePart) -> Self {
        Part::File(part)
    }
}

impl From<DataPart> for Part {
    fn from(part: DataPart) -> Self {
        Part::Data(part)
    }
}

/// A single message in the conversation between a user and an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The type of this object, used as a discriminator. Always 'message' for a Message.
    #[serde(default)]
    pub kind: MessageKind,
    /// Identifies the sender of the message
    pub role: MessageRole,
    /// An array of content parts that form the message body
    pub parts: Vec<Part>,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// The URIs of extensions that are relevant to this message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
    /// A list of other task IDs that this message references for additional context
    #[serde(skip_serializing_if = "Option::is_none", rename = "referenceTaskIds")]
    pub reference_task_ids: Option<Vec<String>>,
    /// A unique identifier for the message, typically a UUID, generated by the sender
    #[serde(rename = "messageId")]
    pub message_id: String,
    /// The ID of the task this message is part of
    #[serde(skip_serializing_if = "Option::is_none", rename = "taskId")]
    pub task_id: Option<String>,
    /// The context ID for this message, used to group related interactions
    #[serde(skip_serializing_if = "Option::is_none", rename = "contextId")]
    pub context_id: Option<String>,
}

impl Message {
    /// Create a new message from the user
    pub fn new_user(parts: Vec<Part>) -> Self {
        Self {
            kind: MessageKind::Message,
            role: MessageRole::User,
            parts,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
            message_id: Uuid::new_v4().to_string(),
            task_id: None,
            context_id: None,
        }
    }

    /// Create a new message from the agent
    pub fn new_agent(parts: Vec<Part>) -> Self {
        Self {
            kind: MessageKind::Message,
            role: MessageRole::Agent,
            parts,
            metadata: None,
            extensions: None,
            reference_task_ids: None,
            message_id: Uuid::new_v4().to_string(),
            task_id: None,
            context_id: None,
        }
    }
}

/// Represents the status of a task at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatus {
    /// The current state of the task's lifecycle
    pub state: TaskState,
    /// An optional, human-readable message providing more details about the current status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<Message>,
    /// An ISO 8601 datetime string indicating when this status was recorded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl TaskStatus {
    /// Create a new task status
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    /// Add a message to the status update
    pub fn with_message(mut self, message: Message) -> Self {
        self.message = Some(message);
        self
    }
}

/// Represents a file, data structure, or other resource generated by an agent during a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// A unique identifier for the artifact within the scope of the task
    #[serde(rename = "artifactId")]
    pub artifact_id: String,
    /// An optional, human-readable name for the artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An optional, human-readable description of the artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// An array of content parts that make up the artifact
    pub parts: Vec<Part>,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// The URIs of extensions that are relevant to this artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<String>>,
}

impl Artifact {
    /// Create a new artifact
    pub fn new(parts: Vec<Part>) -> Self {
        Self {
            artifact_id: Uuid::new_v4().to_string(),
            name: None,
            description: None,
            parts,
            metadata: None,
            extensions: None,
        }
    }

    /// Set the artifact name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the artifact description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Represents a single, stateful operation or conversation between a client and an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// A unique identifier for the task, generated by the server
    pub id: String,
    /// A server-generated unique identifier for maintaining context across multiple related tasks
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The current status of the task, including its state and a descriptive message
    pub status: TaskStatus,
    /// An array of messages exchanged during the task, representing the conversation history
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Message>>,
    /// A collection of artifacts generated by the agent during the execution of the task
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Artifact>>,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    /// The type discriminator for this object
    #[serde(rename = "kind")]
    pub kind: TaskKind,
}

/// Task kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum TaskKind {
    /// Standard task
    #[default]
    Task,
}

impl Task {
    /// Create a new task
    pub fn new(context_id: String, status: TaskStatus) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            context_id,
            status,
            history: None,
            artifacts: None,
            metadata: None,
            kind: TaskKind::Task,
        }
    }

    /// Add message history to the task
    pub fn with_history(mut self, history: Vec<Message>) -> Self {
        self.history = Some(history);
        self
    }

    /// Add artifacts to the task
    pub fn with_artifacts(mut self, artifacts: Vec<Artifact>) -> Self {
        self.artifacts = Some(artifacts);
        self
    }
}

/// An event sent by the agent to notify the client of a change in a task's status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStatusUpdateEvent {
    /// The ID of the task that was updated
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The context ID associated with the task
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The type discriminator for this event
    #[serde(rename = "kind")]
    pub kind: StatusUpdateKind,
    /// The new status of the task
    pub status: TaskStatus,
    /// If true, this is the final event in the stream for this interaction
    #[serde(rename = "final")]
    pub is_final: bool,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Status update kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum StatusUpdateKind {
    /// Task status update
    #[default]
    StatusUpdate,
}

/// An event sent by the agent to notify the client that an artifact has been generated or updated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskArtifactUpdateEvent {
    /// The ID of the task this artifact belongs to
    #[serde(rename = "taskId")]
    pub task_id: String,
    /// The context ID associated with the task
    #[serde(rename = "contextId")]
    pub context_id: String,
    /// The type discriminator for this event
    #[serde(rename = "kind")]
    pub kind: ArtifactUpdateKind,
    /// The artifact that was generated or updated
    pub artifact: Artifact,
    /// If true, the content of this artifact should be appended to a previously sent artifact with the same ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append: Option<bool>,
    /// If true, this is the final chunk of the artifact
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastChunk")]
    pub last_chunk: Option<bool>,
    /// Optional metadata for extensions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Artifact update kind discriminator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum ArtifactUpdateKind {
    /// Task artifact update
    #[default]
    ArtifactUpdate,
}

/// Defines authentication details for a push notification endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationAuthenticationInfo {
    /// A list of supported authentication schemes (e.g., 'Basic', 'Bearer')
    pub schemes: Vec<String>,
    /// Optional credentials required by the push notification endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credentials: Option<String>,
}

/// Configuration for setting up push notifications for task updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotificationConfig {
    /// A unique identifier for the push notification configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The callback URL where the agent should send push notifications
    pub url: String,
    /// A unique token for this task or session to validate incoming push notifications
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Optional authentication details for the agent to use when calling the notification URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<PushNotificationAuthenticationInfo>,
}

/// Represents the service provider of an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProvider {
    /// The name of the agent provider's organization
    pub organization: String,
    /// A URL for the agent provider's website or relevant documentation
    pub url: String,
}

/// A declaration of a protocol extension supported by an Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentExtension {
    /// The unique URI identifying the extension
    pub uri: String,
    /// A human-readable description of how this agent uses the extension
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// If true, the client must understand and comply with the extension's requirements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    /// Optional, extension-specific configuration parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<HashMap<String, serde_json::Value>>,
}

/// Defines optional capabilities supported by an agent
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    /// Indicates if the agent supports Server-Sent Events (SSE) for streaming responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    /// Indicates if the agent supports sending push notifications for asynchronous task updates
    #[serde(skip_serializing_if = "Option::is_none", rename = "pushNotifications")]
    pub push_notifications: Option<bool>,
    /// Indicates if the agent provides a history of state transitions for a task
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "stateTransitionHistory"
    )]
    pub state_transition_history: Option<bool>,
    /// A list of protocol extensions supported by the agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Vec<AgentExtension>>,
}

/// Security scheme types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityScheme {
    /// API key authentication
    #[serde(rename = "apiKey")]
    ApiKey {
        /// Name of the API key parameter
        name: String,
        /// Location of the API key (query, header, cookie)
        #[serde(rename = "in")]
        location: String,
    },
    /// HTTP authentication
    Http {
        /// HTTP authentication scheme (basic, bearer, etc.)
        scheme: String,
        /// Format of the bearer token
        #[serde(skip_serializing_if = "Option::is_none", rename = "bearerFormat")]
        bearer_format: Option<String>,
    },
    /// OAuth 2.0 authentication
    #[serde(rename = "oauth2")]
    OAuth2 {
        /// OAuth 2.0 flow configurations
        flows: serde_json::Value,
    },
    /// OpenID Connect authentication
    #[serde(rename = "openIdConnect")]
    OpenIdConnect {
        /// OpenID Connect discovery URL
        #[serde(rename = "openIdConnectUrl")]
        open_id_connect_url: String,
    },
    /// Mutual TLS authentication
    #[serde(rename = "mutualTLS")]
    MutualTls {},
}

/// Represents a distinct capability or function that an agent can perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    /// A unique identifier for the agent's skill
    pub id: String,
    /// A human-readable name for the skill
    pub name: String,
    /// A detailed description of the skill
    pub description: String,
    /// A set of keywords describing the skill's capabilities
    pub tags: Vec<String>,
    /// Example prompts or scenarios that this skill can handle
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<String>>,
    /// The set of supported input MIME types for this skill
    #[serde(skip_serializing_if = "Option::is_none", rename = "inputModes")]
    pub input_modes: Option<Vec<String>>,
    /// The set of supported output MIME types for this skill
    #[serde(skip_serializing_if = "Option::is_none", rename = "outputModes")]
    pub output_modes: Option<Vec<String>>,
    /// Security schemes necessary for the agent to leverage this skill
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
}

/// Declares a combination of a target URL and a transport protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInterface {
    /// The URL where this interface is available
    pub url: String,
    /// The transport protocol supported at this URL
    pub transport: TransportProtocol,
}

/// JSON Web Signature for an AgentCard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardSignature {
    /// The protected JWS header, Base64url-encoded
    pub protected: String,
    /// The computed signature, Base64url-encoded
    pub signature: String,
    /// The unprotected JWS header values
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, serde_json::Value>>,
}

/// The AgentCard is a self-describing manifest for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// The version of the A2A protocol this agent supports
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    /// A human-readable name for the agent
    pub name: String,
    /// A human-readable description of the agent
    pub description: String,
    /// The preferred endpoint URL for interacting with the agent
    pub url: String,
    /// The transport protocol for the preferred endpoint
    #[serde(skip_serializing_if = "Option::is_none", rename = "preferredTransport")]
    pub preferred_transport: Option<TransportProtocol>,
    /// A list of additional supported interfaces
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "additionalInterfaces"
    )]
    pub additional_interfaces: Option<Vec<AgentInterface>>,
    /// An optional URL to an icon for the agent
    #[serde(skip_serializing_if = "Option::is_none", rename = "iconUrl")]
    pub icon_url: Option<String>,
    /// Information about the agent's service provider
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<AgentProvider>,
    /// The agent's own version number
    pub version: String,
    /// An optional URL to the agent's documentation
    #[serde(skip_serializing_if = "Option::is_none", rename = "documentationUrl")]
    pub documentation_url: Option<String>,
    /// A declaration of optional capabilities supported by the agent
    pub capabilities: AgentCapabilities,
    /// Security schemes available to authorize requests
    #[serde(skip_serializing_if = "Option::is_none", rename = "securitySchemes")]
    pub security_schemes: Option<HashMap<String, SecurityScheme>>,
    /// List of security requirement objects that apply to all agent interactions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<Vec<HashMap<String, Vec<String>>>>,
    /// Default set of supported input MIME types for all skills
    #[serde(rename = "defaultInputModes")]
    pub default_input_modes: Vec<String>,
    /// Default set of supported output MIME types for all skills
    #[serde(rename = "defaultOutputModes")]
    pub default_output_modes: Vec<String>,
    /// The set of skills that the agent can perform
    pub skills: Vec<AgentSkill>,
    /// If true, the agent can provide an extended agent card with additional details
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "supportsAuthenticatedExtendedCard"
    )]
    pub supports_authenticated_extended_card: Option<bool>,
    /// JSON Web Signatures computed for this AgentCard
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<AgentCardSignature>>,
}

impl AgentCard {
    /// Create a new agent card
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        url: impl Into<String>,
    ) -> Self {
        Self {
            protocol_version: "0.3.0".to_string(),
            name: name.into(),
            description: description.into(),
            url: url.into(),
            preferred_transport: Some(TransportProtocol::JsonRpc),
            additional_interfaces: None,
            icon_url: None,
            provider: None,
            version: "1.0.0".to_string(),
            documentation_url: None,
            capabilities: AgentCapabilities::default(),
            security_schemes: None,
            security: None,
            default_input_modes: vec!["application/json".to_string(), "text/plain".to_string()],
            default_output_modes: vec!["application/json".to_string(), "text/plain".to_string()],
            skills: Vec::new(),
            supports_authenticated_extended_card: None,
            signatures: None,
        }
    }
}
