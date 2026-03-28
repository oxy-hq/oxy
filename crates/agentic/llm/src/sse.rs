use serde::Deserialize;

// ── SSE helpers ───────────────────────────────────────────────────────────────

/// Advance `buf` past the next complete SSE event block (separated by `\n\n`
/// or `\r\n\r\n`).  Returns the event block text on success, `None` when no
/// complete event is buffered yet.
pub(super) fn pop_sse_event(buf: &mut String) -> Option<String> {
    let (end, consume) = if let Some(p) = buf.find("\n\n") {
        (p, p + 2)
    } else if let Some(p) = buf.find("\r\n\r\n") {
        (p, p + 4)
    } else {
        return None;
    };
    let event = buf[..end].to_string();
    *buf = buf[consume..].to_string();
    Some(event)
}

/// Extract the `data:` payload from an SSE event block.
pub(super) fn sse_data(event: &str) -> Option<&str> {
    event.lines().find_map(|l| l.strip_prefix("data: "))
}

/// Extract the `event:` type from an SSE event block.
///
/// The OpenAI Responses API tags each SSE chunk with an `event:` line
/// (e.g. `event: response.output_text.delta`).  Returns `None` when the
/// block has no `event:` line (Chat Completions / Anthropic style).
pub(super) fn sse_event_type(event: &str) -> Option<&str> {
    event.lines().find_map(|l| l.strip_prefix("event: "))
}

// ── Shared wire types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(super) struct ApiErrorBody {
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct ApiError {
    pub(super) error: ApiErrorBody,
}
