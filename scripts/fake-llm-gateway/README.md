# fake-llm-gateway

A dependency-free fake OpenAI-compatible gateway for testing **which endpoint
Oxy calls** and **whether a resolved API key reaches the wire** — without a real
provider account.

Stands in for LangDock, Groq, OpenRouter, vLLM, LM Studio, and similar. It logs
every request, so your assertion is what the gateway *received*, not what the
agent eventually answered.

```bash
python3 scripts/fake-llm-gateway/gateway.py
```

| Route | Behaviour |
| ----- | --------- |
| `POST /v1/responses` | `404` by default (`RESPONSES_MODE=ok` to serve it) |
| `POST /v1/chat/completions` | `401` without a key, `401` on the wrong key, `200` + SSE on the right one |

| Env | Default | Meaning |
| --- | ------- | ------- |
| `EXPECTED_KEY` | `sk-smoke-test` | Key that must arrive as `Authorization: Bearer …` |
| `PORT` | `8099` | Listen port |
| `RESPONSES_MODE` | `404` | `404` (third-party gateway) or `ok` (real-OpenAI-shaped) |

## Why this exists

`vendor: openai` targets the **Responses API** (`{api_url}/responses`), while
`vendor: openai_compat` targets **Chat Completions**
(`{api_url}/chat/completions`). Most third-party gateways implement only the
latter, so pointing `vendor: openai` at one yields a 404 that looks like an auth
problem but isn't. This gateway reproduces that split on demand.

It also asserts something a real provider cannot: because it knows the expected
key, a `200` proves the value resolved from `key_var` actually reached the
`Authorization` header — end to end through the secret store.

## Usage

Start the gateway, then in a scratch Oxy project:

```yaml
# config.yml
models:
  - name: compat-test
    vendor: openai_compat
    model_ref: fake-model
    key_var: FAKE_GATEWAY_KEY
    api_url: http://127.0.0.1:8099/v1
```

```yaml
# agents/smoke.agentic.yml
llm:
  ref: compat-test
```

```bash
export FAKE_GATEWAY_KEY=sk-smoke-test
```

Ask the agent anything, then read the gateway log.

### Cases worth covering

| Config | Expected in the gateway log |
| ------ | --------------------------- |
| `vendor: openai_compat` | `POST /v1/chat/completions`, key present, `200` |
| `vendor: openai` | `POST /v1/responses`, `404` |
| `vendor: openai` + `llm.vendor: open_ai_compat` in the agent | `POST /v1/chat/completions`, key present, `200` |
| Wrong `FAKE_GATEWAY_KEY` | `401` — proves the value came from `key_var` |

The third row is the protocol override: an explicit `llm.vendor` beats the
vendor inherited from `llm.ref:`, so a `config.yml` model can supply the managed
secret and `api_url` while the agent picks the wire protocol.

> **The agent run itself will usually fail after the HTTP call.** The gateway
> returns canned prose rather than tool calls, so the analytics FSM won't get
> far. That's expected — this tool tests transport, not reasoning. To exercise
> the full pipeline over Chat Completions, point `api_url` at a local Ollama
> (`http://localhost:11434/v1`) instead; Ollama runs real inference but ignores
> auth, so the two are complementary.

## Quick check without Oxy

```bash
curl -s -o /dev/null -w '%{http_code}\n' -X POST localhost:8099/v1/responses
curl -s -X POST localhost:8099/v1/chat/completions \
  -H 'authorization: Bearer sk-smoke-test' -d '{}'
```
