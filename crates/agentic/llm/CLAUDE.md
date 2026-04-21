# agentic-llm

Unified LLM provider abstraction with token-level streaming and extended thinking support.

## Providers

| Provider | Type | Config |
| ---------- | ------ | -------- |
| Anthropic | `AnthropicProvider` | API key + model name |
| OpenAI | `OpenAiProvider` | API key + model name + optional base URL |
| OpenAI-compatible | `OpenAiCompatProvider` | API key + model + base URL (Ollama, etc.) |

## Key Types

```rust
pub trait LlmProvider: Send + Sync {
    async fn stream(&self, system_prompt, messages, tools, thinking_config)
        -> Result<Stream<Chunk>>;
}

pub struct LlmClient {
    // Wraps a provider with tool-loop orchestration
    pub async fn run_with_tools(&self, ...) -> Result<LlmOutput>;
}

pub struct ThinkingConfig {
    pub enabled: bool,
    pub budget_tokens: Option<u32>,
    pub effort: ReasoningEffort,  // Low | Medium | High
}
```

## Thinking Support

Extended thinking (reasoning) uses opaque encrypted blobs:

- **Must** be passed back verbatim in subsequent tool-use loops
- **Must NOT** cross FSM state boundaries (discarded on state transition)
- Controlled by `ThinkingConfig` from the agent YAML

## Events

`LlmClient::run_with_tools` emits events through `EventStream`:

- `CoreEvent::LlmStart` / `LlmToken` / `LlmEnd` — per HTTP round (each provider.stream() call)
- `CoreEvent::ThinkingStart` / `ThinkingToken` / `ThinkingEnd` — per thinking block
- `CoreEvent::ToolCall` / `ToolResult` — per tool invocation

## Rules

- Infrastructure crate — may be imported by any domain.
- Does NOT depend on runtime, pipeline, or HTTP.
- Provider selection is done in `agentic-pipeline::platform::ProjectContext::resolve_model()`, not here.
