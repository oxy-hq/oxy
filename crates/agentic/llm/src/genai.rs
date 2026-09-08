//! OpenTelemetry **GenAI semantic conventions** for every model call.
//!
//! One span per HTTP round against a provider, named `chat <model>` for the
//! OTel exporter (`otel.name`) while the product observability layer keeps
//! its own vocabulary on the same span (`oxy.name = "llm.call"`,
//! `oxy.span_type = "llm"`). The two never disagree because they are one
//! span: what HyperDX sees as `gen_ai.usage.output_tokens` is what the tenant
//! Traces console sums as cost.
//!
//! The attribute names are pinned to
//! `open-telemetry/semantic-conventions-genai@94f432d7126f` (status:
//! *Development*, no tagged release yet — so a rename upstream is a deliberate
//! edit here, never an implicit one). Everything **Required** and
//! **Recommended** for an inference span is emitted when the provider knows
//! it; the **Opt-In** content attributes (`gen_ai.input.messages`,
//! `gen_ai.system_instructions`, `gen_ai.output.messages`) are recorded only
//! when [`CAPTURE_CONTENT_ENV`] is `true` — off by default, because a prompt
//! carries tenant data and the span lands in **two** stores: the platform
//! trace store behind the OTel collector (where a `transform` processor can
//! strip content again) and the tenant-visible product store, which
//! `oxy-observability` writes directly with no collector in the path. Opting
//! in is therefore a decision about the tenant console, not only HyperDX.
//! Even when on, tool-call arguments and tool results are stripped from both
//! the input history and the output ([`redact_tool_payloads`]).
//!
//! [`observe`] is the only place usage, finish reason, time-to-first-chunk and
//! the error class are recorded, so the three call sites in [`LlmClient`]
//! cannot drift apart.
//!
//! [`LlmClient`]: crate::LlmClient

use std::pin::Pin;
use std::sync::OnceLock;
use std::time::Instant;

use futures_core::Stream;
use tracing::{Span, field::Empty};

use crate::{Chunk, LlmError, LlmProvider, StopReason, Usage};

/// `gen_ai.operation.name` for every call this crate makes: all three
/// providers speak a chat-completion shaped API, tool calls included.
pub const OPERATION_CHAT: &str = "chat";

/// Environment variable that turns on the Opt-In content attributes. Any
/// value other than `true` / `1` (case-insensitive) leaves them off.
pub const CAPTURE_CONTENT_ENV: &str = "OXY_GENAI_CAPTURE_CONTENT";

/// Upper bound on a captured content attribute, so an opted-in deployment
/// never ships a multi-megabyte history on one span.
pub const CONTENT_MAX_BYTES: usize = 64 * 1024;

/// A chunk stream as returned by [`LlmProvider::stream`].
pub type ChunkStream = Pin<Box<dyn Stream<Item = Result<Chunk, LlmError>> + Send>>;

/// Who is asking, for per-tenant accounting and conversation correlation.
///
/// Every field is optional: the client is built in several places and not
/// all of them know the run. Set what is known with
/// [`LlmClient::with_genai_context`](crate::LlmClient::with_genai_context);
/// absent fields are simply not recorded.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GenAiContext {
    /// `gen_ai.conversation.id` — the thread / run the messages belong to.
    pub conversation_id: Option<String>,
    /// `gen_ai.agent.name` — the agent definition driving the call.
    pub agent_name: Option<String>,
    /// `oxy.org_id`, the tenant.
    pub org_id: Option<String>,
    /// `oxy.workspace_id`.
    pub workspace_id: Option<String>,
    /// `oxy.project_id`.
    pub project_id: Option<String>,
}

/// What the caller is about to send, as far as the span needs to know.
pub(crate) struct InferenceRequest<'a> {
    pub system: &'a str,
    pub messages: &'a [serde_json::Value],
    pub max_tokens: Option<u32>,
    pub tool_count: usize,
    /// A response schema was attached, so the provider is constrained to
    /// JSON (`gen_ai.output.type = "json"`).
    pub structured_output: bool,
    /// Tool-loop bookkeeping; `None` on the single-shot paths.
    pub state: Option<&'a str>,
    pub round: Option<u32>,
}

/// Whether the Opt-In content attributes are recorded. Read once per process.
pub fn content_capture_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(CAPTURE_CONTENT_ENV)
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false)
    })
}

/// The semconv provider name for an endpoint host, falling back to
/// `default` when the host is not one of the well-known services. A gateway
/// in front of OpenAI still speaks OpenAI's format, so the caller's default
/// is the provider it was built for, not `"unknown"`.
pub fn provider_name_for_host(host: &str, default: &'static str) -> &'static str {
    let host = host.to_ascii_lowercase();
    if host.ends_with("openai.azure.com") || host.ends_with("cognitiveservices.azure.com") {
        "azure.ai.openai"
    } else if host == "api.openai.com" {
        "openai"
    } else if host == "api.anthropic.com" {
        "anthropic"
    } else if host.ends_with("googleapis.com") {
        "gcp.gemini"
    } else if host.ends_with("amazonaws.com") {
        "aws.bedrock"
    } else {
        default
    }
}

/// [`provider_name_for_host`] applied to a URL; the default when it does not
/// parse. Providers call this once, in their constructors.
pub fn provider_name_for_url(url: &str, default: &'static str) -> &'static str {
    match server_endpoint(url) {
        Some((host, _)) => provider_name_for_host(&host, default),
        None => default,
    }
}

/// `(server.address, server.port)` for a base URL, or `None` when it does not
/// parse. The port is the URL's explicit or scheme-default port.
pub fn server_endpoint(url: &str) -> Option<(String, u16)> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    let port = parsed.port_or_known_default()?;
    Some((host, port))
}

/// Build the span for one inference round. Every attribute that is recorded
/// later by [`observe`] is declared here as [`Empty`]; tracing rejects
/// records for undeclared fields.
pub(crate) fn inference_span(
    provider: &dyn LlmProvider,
    ctx: &GenAiContext,
    req: &InferenceRequest<'_>,
) -> Span {
    let model = provider.model_name();
    let span = tracing::info_span!(
        "llm_round",
        otel.name = %format!("{OPERATION_CHAT} {model}"),
        otel.kind = "client",
        otel.status_code = Empty,
        oxy.name = "llm.call",
        oxy.span_type = "llm",
        gen_ai.operation.name = OPERATION_CHAT,
        gen_ai.provider.name = provider.provider_name(),
        gen_ai.request.model = model,
        gen_ai.request.max_tokens = Empty,
        gen_ai.request.stream = true,
        gen_ai.output.type = Empty,
        gen_ai.conversation.id = Empty,
        gen_ai.agent.name = Empty,
        gen_ai.response.finish_reasons = Empty,
        // Spec attribute at the pinned commit: "Recommended if the request
        // was a streaming request", type double, seconds from the request.
        gen_ai.response.time_to_first_chunk = Empty,
        gen_ai.usage.input_tokens = Empty,
        gen_ai.usage.output_tokens = Empty,
        gen_ai.usage.cache_read.input_tokens = Empty,
        gen_ai.usage.cache_write.input_tokens = Empty,
        gen_ai.input.messages = Empty,
        gen_ai.system_instructions = Empty,
        gen_ai.output.messages = Empty,
        server.address = Empty,
        server.port = Empty,
        error.type = Empty,
        oxy.org_id = Empty,
        oxy.workspace_id = Empty,
        oxy.project_id = Empty,
        oxy.tool_count = req.tool_count as u64,
        llm.state = Empty,
        llm.round = Empty,
    );
    if let Some(max) = req.max_tokens {
        span.record("gen_ai.request.max_tokens", u64::from(max));
    }
    if req.structured_output {
        span.record("gen_ai.output.type", "json");
    }
    if let Some((host, port)) = provider.endpoint().and_then(server_endpoint) {
        span.record("server.address", host.as_str());
        span.record("server.port", u64::from(port));
    }
    if let Some(state) = req.state {
        span.record("llm.state", state);
    }
    if let Some(round) = req.round {
        span.record("llm.round", u64::from(round));
    }
    record_context(&span, ctx);
    if content_capture_enabled() {
        span.record("gen_ai.system_instructions", truncated(req.system).as_str());
        let messages =
            serde_json::to_string(&redact_tool_payloads(req.messages)).unwrap_or_default();
        span.record("gen_ai.input.messages", truncated(&messages).as_str());
    }
    span
}

fn record_context(span: &Span, ctx: &GenAiContext) {
    if let Some(v) = &ctx.conversation_id {
        span.record("gen_ai.conversation.id", v.as_str());
    }
    if let Some(v) = &ctx.agent_name {
        span.record("gen_ai.agent.name", v.as_str());
    }
    if let Some(v) = &ctx.org_id {
        span.record("oxy.org_id", v.as_str());
    }
    if let Some(v) = &ctx.workspace_id {
        span.record("oxy.workspace_id", v.as_str());
    }
    if let Some(v) = &ctx.project_id {
        span.record("oxy.project_id", v.as_str());
    }
}

/// Wrap a provider stream so the span learns what the response was: time to
/// first chunk (measured from `started`, taken *before* the request was
/// sent, so connect, upload and provider queueing count), usage and finish
/// reason on `Done`, the error class on `Err`, and (opt-in) the output
/// messages once the stream ends. Chunks pass through untouched.
pub(crate) fn observe(span: Span, started: Instant, inner: ChunkStream) -> ChunkStream {
    let capture = content_capture_enabled();
    Box::pin(async_stream::stream! {
        use tokio_stream::StreamExt as _;
        let mut first = true;
        let mut text = String::new();
        let mut tool_names: Vec<String> = Vec::new();
        let mut inner = inner;
        while let Some(item) = inner.next().await {
            if first {
                first = false;
                span.record(
                    "gen_ai.response.time_to_first_chunk",
                    started.elapsed().as_secs_f64(),
                );
            }
            match &item {
                Ok(Chunk::Done(usage)) => record_usage(&span, usage),
                Ok(Chunk::Text(t)) if capture => text.push_str(t),
                Ok(Chunk::ToolCall(tc)) if capture => tool_names.push(tc.name.clone()),
                Err(e) => record_error(&span, e),
                _ => {}
            }
            yield item;
        }
        if capture {
            let out = output_messages(&text, &tool_names);
            span.record("gen_ai.output.messages", truncated(&out).as_str());
        }
    })
}

/// Record the failure of the request itself (before any chunk arrived).
pub(crate) fn record_error(span: &Span, err: &LlmError) {
    span.record("error.type", error_type(err));
    span.record("otel.status_code", "ERROR");
}

fn record_usage(span: &Span, usage: &Usage) {
    span.record("gen_ai.usage.input_tokens", usage.input_tokens as u64);
    span.record("gen_ai.usage.output_tokens", usage.output_tokens as u64);
    span.record(
        "gen_ai.usage.cache_read.input_tokens",
        usage.cache_read_input_tokens as u64,
    );
    span.record(
        "gen_ai.usage.cache_write.input_tokens",
        usage.cache_creation_input_tokens as u64,
    );
    span.record(
        "gen_ai.response.finish_reasons",
        finish_reasons_json(&usage.stop_reason).as_str(),
    );
}

/// The semconv `gen_ai.response.finish_reasons` value: a JSON array, because
/// tracing fields are scalars and the convention's type is `string[]`.
pub fn finish_reasons_json(reason: &StopReason) -> String {
    let word = match reason {
        StopReason::EndTurn => "stop",
        StopReason::MaxTokens => "length",
        StopReason::ToolUse => "tool_calls",
    };
    format!("[\"{word}\"]")
}

/// `error.type`: a low-cardinality class, never the message.
pub fn error_type(err: &LlmError) -> &'static str {
    match err {
        LlmError::Http(_) => "http",
        LlmError::Transient(_) => "transient",
        LlmError::Auth(_) => "auth",
        LlmError::RateLimit { .. } => "rate_limit",
        LlmError::Parse(_) => "parse",
        LlmError::EmptyResponse { .. } => "empty_response",
        LlmError::Suspended { .. } => "suspended",
        LlmError::MaxTokensReached { .. } => "max_tokens",
        LlmError::MaxToolRoundsReached { .. } => "max_tool_rounds",
    }
}

/// Objects in a provider-native history that carry a tool's arguments or
/// result. Anthropic (`tool_use` / `tool_result`), OpenAI Chat
/// (`tool_calls[].type = "function"`, `role = "tool"`), OpenAI Responses
/// (`function_call` / `function_call_output`) and the semconv shapes.
const TOOL_PAYLOAD_TYPES: &[&str] = &[
    "tool_use",
    "tool_result",
    "function",
    "function_call",
    "function_call_output",
    "tool_call",
    "tool_call_response",
];

/// The identifiers a redacted tool object keeps.
const TOOL_IDENTIFIER_KEYS: &[&str] = &[
    "type",
    "role",
    "id",
    "name",
    "call_id",
    "tool_use_id",
    "tool_call_id",
];

/// Strip tool-call arguments and tool results from a provider-native message
/// history, keeping roles, text and tool *names*. Round N+1's input replays
/// round N's tool traffic verbatim, so without this the output redaction
/// would buy nothing past the first round.
pub fn redact_tool_payloads(messages: &[serde_json::Value]) -> serde_json::Value {
    serde_json::Value::Array(messages.iter().map(redact_value).collect())
}

fn redact_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::Object(map) => {
            let ty = map.get("type").and_then(Value::as_str).unwrap_or("");
            let role = map.get("role").and_then(Value::as_str).unwrap_or("");
            if TOOL_PAYLOAD_TYPES.contains(&ty) || role == "tool" {
                let mut out = serde_json::Map::new();
                for key in TOOL_IDENTIFIER_KEYS {
                    if let Some(x) = map.get(*key) {
                        out.insert((*key).to_string(), x.clone());
                    }
                }
                if let Some(name) = map
                    .get("function")
                    .and_then(Value::as_object)
                    .and_then(|f| f.get("name"))
                {
                    out.insert("function".into(), serde_json::json!({ "name": name }));
                }
                out.insert("redacted".into(), Value::Bool(true));
                Value::Object(out)
            } else {
                Value::Object(
                    map.iter()
                        .map(|(k, v)| (k.clone(), redact_value(v)))
                        .collect(),
                )
            }
        }
        other => other.clone(),
    }
}

/// `gen_ai.output.messages` in the convention's role/parts shape, built from
/// what the stream yielded. Tool calls carry the name only: arguments are
/// tenant data even when content capture is on.
fn output_messages(text: &str, tool_names: &[String]) -> String {
    let mut parts: Vec<serde_json::Value> = Vec::new();
    if !text.is_empty() {
        parts.push(serde_json::json!({"type": "text", "content": text}));
    }
    for name in tool_names {
        parts.push(serde_json::json!({"type": "tool_call", "name": name}));
    }
    serde_json::json!([{"role": "assistant", "parts": parts}]).to_string()
}

/// Cut at a char boundary under [`CONTENT_MAX_BYTES`], marking the cut so a
/// reader knows the attribute is partial.
fn truncated(s: &str) -> String {
    if s.len() <= CONTENT_MAX_BYTES {
        return s.to_string();
    }
    let cut = s
        .char_indices()
        .take_while(|(i, _)| *i < CONTENT_MAX_BYTES)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    format!("{}… ({} bytes)", &s[..cut], s.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::{Layer, Registry};

    /// Captures every field recorded on every span, keyed by span name.
    #[derive(Default, Clone)]
    struct Capture(Arc<Mutex<HashMap<String, HashMap<String, String>>>>);

    struct Collect<'a>(&'a mut HashMap<String, String>);
    impl Visit for Collect<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0
                .insert(field.name().to_string(), format!("{value:?}"));
        }
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_u64(&mut self, field: &Field, value: u64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_f64(&mut self, field: &Field, value: f64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_bool(&mut self, field: &Field, value: bool) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
    }

    impl<S: tracing::Subscriber> Layer<S> for Capture {
        fn on_new_span(
            &self,
            attrs: &tracing::span::Attributes<'_>,
            _id: &tracing::span::Id,
            _ctx: Context<'_, S>,
        ) {
            let mut all = self.0.lock().unwrap();
            let fields = all.entry(attrs.metadata().name().to_string()).or_default();
            attrs.record(&mut Collect(fields));
        }
        fn on_record(
            &self,
            _id: &tracing::span::Id,
            values: &tracing::span::Record<'_>,
            _ctx: Context<'_, S>,
        ) {
            let mut all = self.0.lock().unwrap();
            let fields = all.entry("llm_round".to_string()).or_default();
            values.record(&mut Collect(fields));
        }
    }

    struct FakeProvider;
    #[async_trait::async_trait]
    impl LlmProvider for FakeProvider {
        async fn stream(
            &self,
            _: &str,
            _: &str,
            _: &[serde_json::Value],
            _: &[agentic_core::tools::ToolDef],
            _: &crate::ThinkingConfig,
            _: Option<&crate::ResponseSchema>,
            _: Option<u32>,
        ) -> Result<ChunkStream, LlmError> {
            unreachable!()
        }
        fn assistant_message(&self, _: &[crate::ContentBlock]) -> serde_json::Value {
            unreachable!()
        }
        fn tool_result_messages(&self, _: &[(String, String, bool)]) -> Vec<serde_json::Value> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "claude-sonnet-4-6"
        }
        fn provider_name(&self) -> &str {
            "anthropic"
        }
        fn endpoint(&self) -> Option<&str> {
            Some("https://api.anthropic.com/v1")
        }
    }

    fn chunks(items: Vec<Result<Chunk, LlmError>>) -> ChunkStream {
        Box::pin(tokio_stream::iter(items))
    }

    #[tokio::test]
    async fn a_round_records_the_semconv_attributes_and_usage() {
        let cap = Capture::default();
        let subscriber = Registry::default().with(cap.clone());
        let _guard = tracing::subscriber::set_default(subscriber);

        let ctx = GenAiContext {
            conversation_id: Some("thread-1".into()),
            agent_name: Some("sales".into()),
            org_id: Some("org-1".into()),
            ..Default::default()
        };
        let req = InferenceRequest {
            system: "be brief",
            messages: &[],
            max_tokens: Some(512),
            tool_count: 3,
            structured_output: true,
            state: Some("solving"),
            round: Some(2),
        };
        let span = inference_span(&FakeProvider, &ctx, &req);
        let usage = Usage {
            input_tokens: 120,
            output_tokens: 30,
            cache_creation_input_tokens: 5,
            cache_read_input_tokens: 100,
            stop_reason: StopReason::ToolUse,
        };
        let started = Instant::now() - std::time::Duration::from_millis(250);
        let mut s = observe(
            span,
            started,
            chunks(vec![Ok(Chunk::Text("hi".into())), Ok(Chunk::Done(usage))]),
        );
        use tokio_stream::StreamExt as _;
        let mut n = 0;
        while s.next().await.is_some() {
            n += 1;
        }
        assert_eq!(n, 2, "chunks pass through untouched");

        let all = cap.0.lock().unwrap();
        let f = &all["llm_round"];
        assert_eq!(f["otel.name"], "chat claude-sonnet-4-6");
        assert_eq!(f["gen_ai.operation.name"], "chat");
        assert_eq!(f["gen_ai.provider.name"], "anthropic");
        assert_eq!(f["gen_ai.request.model"], "claude-sonnet-4-6");
        assert_eq!(f["gen_ai.request.max_tokens"], "512");
        assert_eq!(f["gen_ai.request.stream"], "true");
        assert_eq!(f["gen_ai.output.type"], "json");
        assert_eq!(f["gen_ai.conversation.id"], "thread-1");
        assert_eq!(f["gen_ai.agent.name"], "sales");
        assert_eq!(f["oxy.org_id"], "org-1");
        assert!(
            !f.contains_key("oxy.project_id"),
            "absent context is not recorded"
        );
        assert_eq!(f["server.address"], "api.anthropic.com");
        assert_eq!(f["server.port"], "443");
        assert_eq!(f["llm.state"], "solving");
        assert_eq!(f["llm.round"], "2");
        assert_eq!(f["gen_ai.usage.input_tokens"], "120");
        assert_eq!(f["gen_ai.usage.output_tokens"], "30");
        assert_eq!(f["gen_ai.usage.cache_read.input_tokens"], "100");
        assert_eq!(f["gen_ai.usage.cache_write.input_tokens"], "5");
        assert_eq!(f["gen_ai.response.finish_reasons"], "[\"tool_calls\"]");
        let ttfc: f64 = f["gen_ai.response.time_to_first_chunk"].parse().unwrap();
        assert!(ttfc >= 0.25, "measured from before the request: {ttfc}");
        assert!(!f.contains_key("error.type"));
        // The product layer's vocabulary rides on the same span.
        assert_eq!(f["oxy.name"], "llm.call");
        assert_eq!(f["oxy.span_type"], "llm");
        // Content is Opt-In and the env is unset here.
        assert!(!f.contains_key("gen_ai.input.messages"));
        assert!(!f.contains_key("gen_ai.system_instructions"));
        assert!(!f.contains_key("gen_ai.output.messages"));
    }

    #[tokio::test]
    async fn a_failed_stream_records_the_error_class_not_the_message() {
        let cap = Capture::default();
        let subscriber = Registry::default().with(cap.clone());
        let _guard = tracing::subscriber::set_default(subscriber);
        let req = InferenceRequest {
            system: "",
            messages: &[],
            max_tokens: None,
            tool_count: 0,
            structured_output: false,
            state: None,
            round: None,
        };
        let span = inference_span(&FakeProvider, &GenAiContext::default(), &req);
        let mut s = observe(
            span,
            Instant::now(),
            chunks(vec![Err(LlmError::RateLimit {
                message: "secret-tenant-detail".into(),
                retry_after: None,
            })]),
        );
        use tokio_stream::StreamExt as _;
        while s.next().await.is_some() {}
        let all = cap.0.lock().unwrap();
        let f = &all["llm_round"];
        assert_eq!(f["error.type"], "rate_limit");
        assert_eq!(f["otel.status_code"], "ERROR");
        assert!(!f.contains_key("gen_ai.request.max_tokens"));
        assert!(!f.contains_key("gen_ai.output.type"));
        assert!(!f.values().any(|v| v.contains("secret-tenant-detail")));
    }

    #[test]
    fn provider_name_maps_well_known_hosts_and_keeps_the_default_for_gateways() {
        assert_eq!(
            provider_name_for_host("myres.openai.azure.com", "openai"),
            "azure.ai.openai"
        );
        assert_eq!(
            provider_name_for_host("api.openai.com", "openai_compat"),
            "openai"
        );
        assert_eq!(
            provider_name_for_host("API.ANTHROPIC.COM", "custom"),
            "anthropic"
        );
        assert_eq!(
            provider_name_for_host("gw.example.internal", "openai"),
            "openai"
        );
        assert_eq!(
            provider_name_for_host("localhost", "openai_compat"),
            "openai_compat"
        );
    }

    #[test]
    fn server_endpoint_uses_the_scheme_default_port() {
        assert_eq!(
            server_endpoint("https://api.openai.com/v1"),
            Some(("api.openai.com".into(), 443))
        );
        assert_eq!(
            server_endpoint("http://localhost:11434/v1"),
            Some(("localhost".into(), 11434))
        );
        assert_eq!(server_endpoint("not a url"), None);
    }

    #[test]
    fn finish_reasons_and_error_types_are_low_cardinality() {
        assert_eq!(finish_reasons_json(&StopReason::EndTurn), "[\"stop\"]");
        assert_eq!(finish_reasons_json(&StopReason::MaxTokens), "[\"length\"]");
        assert_eq!(error_type(&LlmError::Auth("k".into())), "auth");
        assert_eq!(error_type(&LlmError::Parse("x".into())), "parse");
    }

    #[test]
    fn truncation_marks_the_cut_and_respects_char_boundaries() {
        let s = "é".repeat(CONTENT_MAX_BYTES); // 2 bytes each
        let t = truncated(&s);
        assert!(t.ends_with(&format!("… ({} bytes)", s.len())));
        assert!(t.len() < s.len());
        assert_eq!(truncated("short"), "short");
    }

    #[test]
    fn input_history_keeps_text_and_tool_names_but_never_arguments_or_results() {
        let history = vec![
            serde_json::json!({"role": "user", "content": "How many orders?"}),
            // Anthropic shape
            serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Let me check."},
                {"type": "tool_use", "id": "tu_1", "name": "run_sql", "input": {"sql": "select secret from t"}}
            ]}),
            serde_json::json!({"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "tu_1", "content": [{"type": "text", "text": "42 rows of PII"}]}
            ]}),
            // OpenAI Chat shape
            serde_json::json!({"role": "assistant", "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "run_sql", "arguments": "{\"sql\":\"select ssn\"}"}}
            ]}),
            serde_json::json!({"role": "tool", "tool_call_id": "call_1", "content": "ssn=123"}),
            // OpenAI Responses shape
            serde_json::json!({"type": "function_call", "id": "fc_1", "call_id": "c1", "name": "run_sql", "arguments": "{\"sql\":\"x\"}"}),
            serde_json::json!({"type": "function_call_output", "call_id": "c1", "output": "leak"}),
        ];
        let out = redact_tool_payloads(&history);
        let text = out.to_string();
        for leaked in [
            "secret",
            "PII",
            "ssn",
            "leak",
            "arguments",
            "\"input\"",
            "\"output\"",
        ] {
            assert!(!text.contains(leaked), "{leaked} survived: {text}");
        }
        assert!(text.contains("How many orders?"));
        assert!(text.contains("Let me check."));
        assert!(text.contains("run_sql"), "tool names stay");
        assert_eq!(out[1]["content"][1]["redacted"], true);
        assert_eq!(out[3]["tool_calls"][0]["function"]["name"], "run_sql");
        assert_eq!(out[4]["tool_call_id"], "call_1");
        assert!(out[4].get("content").is_none());
        assert_eq!(out[6]["call_id"], "c1");
    }

    #[test]
    fn output_messages_carry_tool_names_only() {
        let out = output_messages("hello", &["get_weather".into()]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v[0]["role"], "assistant");
        assert_eq!(v[0]["parts"][0]["content"], "hello");
        assert_eq!(v[0]["parts"][1]["name"], "get_weather");
        assert!(v[0]["parts"][1].get("arguments").is_none());
    }
}
