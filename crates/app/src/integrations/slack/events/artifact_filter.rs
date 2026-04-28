//! Streaming-safe filter that replaces Oxy's `:::artifact{...}:::` directives
//! with a short inline placeholder ("📎 *title*") before we push the text
//! to Slack. Slack messages get a single "View thread in Oxygen →" footer
//! button, so per-artifact deep links would be redundant noise — and we
//! don't have direct linking to individual tool-call results anyway.
//!
//! The agent emits artifact blocks inline as colon-fenced directives whose
//! length varies (3 or more colons — see `crates/core/src/types.rs`). The
//! Oxy web app renders them with a rich artifact component; Slack has no
//! equivalent, so we swap the raw directive for a compact attribution line
//! pointing back at the full thread in Oxy. Persistence of the structured
//! artifact row itself is handled independently via BlockHandler — this
//! filter only shapes the Slack message body.
//!
//! When no thread URL is configured (Slack misconfigured), the filter falls
//! back to its older drop-silently behaviour so the stream never leaks raw
//! fence markers into the user's view.
//!
//! The filter is stateful because fences can straddle stream chunk
//! boundaries: an opening `:::artifact{…}` can arrive in one flush batch
//! and its matching closer `:::` in the next. `feed` emits only the prefix
//! that's definitely outside any open fence; a half-open fence is buffered
//! and re-joined with the next chunk.

/// State machine for stripping — or, with a thread URL, replacing — artifact
/// directive blocks across streamed chunks.
#[derive(Debug, Default)]
pub struct ArtifactFilter {
    /// Text that arrived after a potential fence start we haven't fully
    /// classified yet (could be the start of `:::artifact{` or just text
    /// containing colons).
    pending: String,
    /// If `Some(n)`, we're currently inside an artifact block opened with
    /// `n` colons and are discarding all input until a matching `:{n}` closer.
    inside_fence_len: Option<usize>,
    /// Set after we've just closed an artifact block with a bare `:::` whose
    /// trailing newline hasn't arrived yet. Swallow exactly one leading `\n`
    /// from the next input to keep the output free of a stray blank line.
    swallow_leading_newline: bool,
    /// Base URL of the Oxy thread this filter is rendering into. When set,
    /// artifacts are replaced with an inline placeholder linking back to
    /// the thread; when `None`, they're dropped entirely (back-compat).
    thread_url: Option<String>,
    /// Parsed attributes of the currently open artifact (populated on
    /// opener, consumed on closer).
    current_attrs: Option<ArtifactAttrs>,
}

/// Minimal parsed view of a `:::artifact{...}` opener — just the bits we
/// want to surface in the Slack placeholder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ArtifactAttrs {
    title: Option<String>,
    kind: Option<String>,
    is_verified: bool,
}

impl ArtifactFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct a filter that replaces artifacts with an inline placeholder
    /// linking to the given thread URL (e.g. `https://app.oxy.tech/threads/<id>`).
    pub fn with_thread_url(thread_url: String) -> Self {
        Self {
            thread_url: Some(thread_url),
            ..Self::default()
        }
    }

    /// Feed the next chunk of streamed text. Returns the portion that's
    /// safe to emit (with any complete artifact blocks removed). Any
    /// unfinished fence is retained internally and will be reprocessed
    /// with the next chunk — or flushed by `finish()` at stream end.
    pub fn feed(&mut self, chunk: &str) -> String {
        self.pending.push_str(chunk);
        // If the previous chunk ended with a close fence that had no
        // trailing newline, and this chunk starts with one, eat it so the
        // artifact block removal leaves no blank line behind.
        if self.swallow_leading_newline {
            self.swallow_leading_newline = false;
            if self.pending.starts_with('\n') {
                self.pending.drain(..1);
            }
        }
        let mut out = String::new();
        self.drain(&mut out, /* at_end = */ false);
        out
    }

    /// Stream is ending. Emit any remaining safe text. If we're still
    /// inside an unclosed artifact block, silently drop it (matches what
    /// the web app does — unclosed fences render as noise).
    pub fn finish(&mut self) -> String {
        let mut out = String::new();
        self.drain(&mut out, /* at_end = */ true);
        out
    }

    /// Advance state against `self.pending`, appending safe text to `out`.
    fn drain(&mut self, out: &mut String, at_end: bool) {
        loop {
            match self.inside_fence_len {
                // Not inside a fence — scan for the next opening or emit text.
                None => {
                    let Some(open_start) = find_artifact_open(&self.pending) else {
                        // No open fence anywhere. But a trailing run of colons
                        // could be the *start* of one that we just haven't
                        // received the full `artifact{` token for yet —
                        // hold them back unless we're finishing.
                        let safe_prefix = trailing_colon_boundary(&self.pending, at_end);
                        out.push_str(&self.pending[..safe_prefix]);
                        self.pending.drain(..safe_prefix);
                        return;
                    };
                    // Emit everything before the fence as normal text.
                    out.push_str(&self.pending[..open_start]);
                    // Figure out the fence length (count the run of colons).
                    let fence_len = count_colons(&self.pending[open_start..]);
                    // Parse `artifact{...}` attributes from the opener so we
                    // can render a placeholder when the closer arrives. If
                    // the `}` hasn't streamed yet we still advance — we just
                    // lose the title; better than blocking the stream.
                    let after_colons = open_start + fence_len;
                    self.current_attrs = parse_opener_attrs(&self.pending[after_colons..]);
                    // Drop both the emitted prefix and the opening fence run
                    // from the buffer, then enter "inside" mode. The
                    // `artifact{...}` body itself will be consumed below when
                    // we drain up to the closer.
                    self.pending.drain(..after_colons);
                    self.inside_fence_len = Some(fence_len);
                }
                // Inside — scan for the matching close; discard the artifact
                // body and either drop or emit a placeholder.
                Some(fence_len) => {
                    let Some((close_end, ate_newline)) = find_close(&self.pending, fence_len)
                    else {
                        // Closer not yet in buffer.
                        if at_end {
                            // Unclosed at stream end — silently drop.
                            self.pending.clear();
                            self.inside_fence_len = None;
                            self.current_attrs = None;
                        }
                        // Either way, nothing more to emit now.
                        return;
                    };
                    // Discard the artifact body + closer.
                    self.pending.drain(..close_end);
                    self.inside_fence_len = None;
                    // Emit a placeholder referencing the artifact, if we
                    // have both a URL and at least a title to show.
                    let attrs = self.current_attrs.take();
                    if let (Some(url), Some(attrs)) = (self.thread_url.as_deref(), attrs)
                        && let Some(placeholder) = render_placeholder(&attrs, url)
                    {
                        out.push_str(&placeholder);
                    }
                    // If the closer wasn't followed by a newline inside this
                    // chunk, arrange to eat one at the start of the next feed
                    // (prevents a stray blank line when the closer and its
                    // newline land in separate chunks).
                    if !ate_newline {
                        self.swallow_leading_newline = true;
                    }
                    // Loop continues; there may be more artifacts back-to-back.
                }
            }
        }
    }
}

/// Find the byte offset of the next `:{3,}artifact` sequence in `s`, if any.
fn find_artifact_open(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i] == b':' {
            let run = count_colons(&s[i..]);
            if run >= 3 && s[i + run..].starts_with("artifact") {
                return Some(i);
            }
            i += run.max(1);
        } else {
            i += 1;
        }
    }
    None
}

/// Count leading colons in `s`. Returns 0 if `s` doesn't start with ':'.
fn count_colons(s: &str) -> usize {
    s.bytes().take_while(|&b| b == b':').count()
}

/// Given we're inside an artifact opened with `fence_len` colons, find the
/// matching `:{fence_len}` closer. Returns `(end_offset, ate_newline)` where
/// `end_offset` is the byte offset just past the closer (and optional trailing
/// newline), and `ate_newline` indicates whether a trailing `\n` was included.
/// Returns None if the closer isn't in `s`.
fn find_close(s: &str, fence_len: usize) -> Option<(usize, bool)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            let run = count_colons(&s[i..]);
            // The closer must be exactly `fence_len` colons — no more, no less
            // (avoid matching a longer run that's actually the start of a
            // nested artifact, though we don't otherwise support nesting).
            if run == fence_len {
                let after = i + run;
                // Consume a trailing newline if present.
                if after < bytes.len() && bytes[after] == b'\n' {
                    return Some((after + 1, true));
                }
                return Some((after, false));
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// Find the byte offset up to which `pending` is "safe to emit" when we're
/// *not* currently inside a fence and no `artifact{` token was found. The
/// only thing that can still turn into an artifact opener is a trailing run
/// of colons; hold those back unless the stream is ending.
fn trailing_colon_boundary(pending: &str, at_end: bool) -> usize {
    if at_end {
        return pending.len();
    }
    let bytes = pending.as_bytes();
    let mut end = pending.len();
    while end > 0 && bytes[end - 1] == b':' {
        end -= 1;
    }
    // Don't strip more than N trailing colons; a full "::::::::::" run is
    // 10+ bytes, so hold back up to, say, 20 bytes of a potential fence.
    let held = pending.len() - end;
    if held > 20 {
        // Give up and emit; this wasn't a fence after all.
        pending.len()
    } else {
        end
    }
}

/// Parse the `artifact{id=... kind=... title=... is_verified=...}` opener
/// attribute block. Input is the text starting at `artifact{...}` (after the
/// colon run). Returns None if no matching `}` is in range (attributes still
/// streaming, or malformed directive).
///
/// Key order is assumed stable (`id`, `kind`, `title`, `is_verified`) — this
/// matches `Block::artifacts_opener` in `crates/core/src/types.rs`. If that
/// format changes in a way that breaks parsing, we fall back to dropping the
/// artifact silently (no placeholder), which is still preferable to
/// surfacing a half-parsed directive.
fn parse_opener_attrs(s: &str) -> Option<ArtifactAttrs> {
    // Must start with "artifact{"
    let after_keyword = s.strip_prefix("artifact{")?;
    // Find the matching `}` (a title cannot contain `}` per the emitter).
    let close = after_keyword.find('}')?;
    let body = &after_keyword[..close];
    // `id` and `kind` are space-delimited; `is_verified=…` is the sentinel
    // we parse backwards from so `title=...` (which may contain spaces) is
    // read as everything between "title=" and " is_verified=".
    let kind = extract_space_delimited(body, "kind=");
    let (title, is_verified) = {
        let title_start = body.find("title=").map(|i| i + "title=".len());
        let verified_start = body.find(" is_verified=");
        match (title_start, verified_start) {
            (Some(t), Some(v)) if t <= v => {
                let title_str = body[t..v].trim();
                let rest = &body[v + " is_verified=".len()..];
                // `is_verified` is the last field — read until end or space.
                let end = rest.find(' ').unwrap_or(rest.len());
                let flag = rest[..end].eq_ignore_ascii_case("true");
                (Some(title_str.to_string()), flag)
            }
            (Some(t), None) => {
                // No is_verified marker → title runs to end of body.
                (Some(body[t..].trim().to_string()), false)
            }
            _ => (None, false),
        }
    };
    Some(ArtifactAttrs {
        title: title.filter(|t| !t.is_empty()),
        kind,
        is_verified,
    })
}

/// Extract the value of a space-delimited `key=...` pair from an attribute body.
fn extract_space_delimited(body: &str, key: &str) -> Option<String> {
    let pos = body.find(key)?;
    let after = &body[pos + key.len()..];
    let end = after.find(' ').unwrap_or(after.len());
    let v = &after[..end];
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

/// Render the inline Slack placeholder for an artifact.
///
/// Visual goals:
/// - **Receded from prose**: wrapped in a `>` blockquote so Slack draws its
///   left border + slightly muted background — clearly secondary to the
///   agent's main response.
/// - **Paragraph-isolated**: leading/trailing `\n\n` produce real paragraph
///   breaks (single `\n` becomes a soft line break in Slack and just looks
///   cramped against neighbouring prose).
/// - **Subtext-styled**: italic, no bold, lowercase action text. The headline
///   call-to-action lives in the footer card; this is just a quiet pointer.
///
/// Output uses GFM markdown — Slack's `chat.appendStream` `markdown_text`
/// field accepts it: `[text](url)` for clickable links, `_text_` for italic.
fn render_placeholder(attrs: &ArtifactAttrs, _thread_url: &str) -> Option<String> {
    // The thread URL is intentionally unused — the footer "View thread in
    // Oxygen →" button is the one and only deep link in the message. Per
    // tool-call links would just clutter the body, and we don't have
    // direct-linking to individual tool-call results anyway.
    let title = attrs.title.as_deref()?;
    let verified = if attrs.is_verified { " ✓" } else { "" };
    Some(format!("\n\n> 📎 _{title}_{verified}\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_plain_text() {
        let mut f = ArtifactFilter::new();
        let out = f.feed("hello world\n");
        assert_eq!(out, "hello world\n");
        assert_eq!(f.finish(), "");
    }

    #[test]
    fn strips_complete_artifact_in_one_chunk() {
        let mut f = ArtifactFilter::new();
        let input = "before\n:::artifact{id=1 kind=semantic_query title=foo is_verified=true}\n:::\nafter\n";
        let out = f.feed(input);
        assert_eq!(out, "before\nafter\n");
        assert_eq!(f.finish(), "");
    }

    #[test]
    fn strips_artifact_with_longer_fences() {
        let mut f = ArtifactFilter::new();
        // The user's report showed 10-colon fences.
        let input = "intro\n::::::::::artifact{id=x kind=y}\n::::::::::\ndone\n";
        let out = f.feed(input);
        assert_eq!(out, "intro\ndone\n");
    }

    #[test]
    fn handles_artifact_split_across_chunks() {
        let mut f = ArtifactFilter::new();
        let mut acc = String::new();
        acc.push_str(&f.feed("prefix "));
        acc.push_str(&f.feed(":::artifact{id=1 kind=k"));
        acc.push_str(&f.feed(" title=t is_verified=true}\n"));
        acc.push_str(&f.feed(":::"));
        acc.push_str(&f.feed("\n more\n"));
        acc.push_str(&f.finish());
        assert_eq!(acc, "prefix  more\n");
    }

    #[test]
    fn holds_trailing_colons_until_resolved() {
        let mut f = ArtifactFilter::new();
        // Colons arrive first — might or might not become an artifact.
        let out1 = f.feed("hello :::");
        // The trailing `:::` should be held back.
        assert_eq!(out1, "hello ");
        // Reveal that it was actually just text (not `artifact{`).
        let out2 = f.feed(" world\n");
        assert_eq!(out2, "::: world\n");
    }

    #[test]
    fn drops_unclosed_artifact_at_finish() {
        let mut f = ArtifactFilter::new();
        let out = f.feed("before\n:::artifact{id=1}\nincomplete...");
        assert_eq!(out, "before\n");
        // Stream ends without a closer — silently drop the rest.
        assert_eq!(f.finish(), "");
    }

    #[test]
    fn strips_multiple_artifacts_back_to_back() {
        let mut f = ArtifactFilter::new();
        let input = "a\n:::artifact{id=1}\n:::\n:::artifact{id=2}\n:::\nb\n";
        let out = f.feed(input);
        assert_eq!(out, "a\nb\n");
    }

    #[test]
    fn does_not_confuse_colons_in_regular_markdown() {
        let mut f = ArtifactFilter::new();
        let out = f.feed("a :: b ::: c\n");
        assert_eq!(out, "a :: b ::: c\n");
    }

    #[test]
    fn parses_opener_attrs_with_verified_true() {
        let attrs = parse_opener_attrs(
            "artifact{id=abc kind=semantic_query title=Sales by Store is_verified=true}\nbody",
        )
        .expect("should parse");
        assert_eq!(attrs.title.as_deref(), Some("Sales by Store"));
        assert_eq!(attrs.kind.as_deref(), Some("semantic_query"));
        assert!(attrs.is_verified);
    }

    #[test]
    fn parses_opener_attrs_without_verified_flag() {
        // When is_verified is missing, fall back to title-runs-to-end-of-body.
        let attrs =
            parse_opener_attrs("artifact{id=abc kind=k title=Untitled}\n").expect("should parse");
        assert_eq!(attrs.title.as_deref(), Some("Untitled"));
        assert!(!attrs.is_verified);
    }

    #[test]
    fn parses_opener_attrs_returns_none_when_body_incomplete() {
        // `}` not yet streamed — should not misparse.
        assert!(parse_opener_attrs("artifact{id=abc kind=sem title=Foo").is_none());
    }

    #[test]
    fn emits_placeholder_without_per_artifact_link() {
        let mut f = ArtifactFilter::with_thread_url("https://app.oxy.tech/threads/T1".to_string());
        let input = "Here it is:\n:::artifact{id=a kind=semantic_query title=Stores by Region is_verified=true}\nsql body\n:::\nmore\n";
        let out = f.feed(input);
        assert!(out.contains("Stores by Region"), "got: {out}");
        // Per-artifact links removed — the footer "View thread in Oxygen →"
        // button is the single deep link.
        assert!(
            !out.contains("[view in Oxy]"),
            "per-artifact link should be gone: {out}"
        );
        assert!(
            !out.contains("https://app.oxy.tech/threads/T1"),
            "thread URL must not appear inline: {out}"
        );
        assert!(!out.contains("**"), "no bold expected: {out}");
        // Blockquote `> ` prefix still isolates the placeholder from surrounding prose.
        assert!(
            out.contains("> 📎"),
            "expected blockquote-prefixed placeholder: {out}"
        );
        assert!(
            out.contains(" ✓"),
            "verified artifact should show check: {out}"
        );
        // The raw directive must not leak.
        assert!(!out.contains(":::artifact"));
        assert!(!out.contains("sql body"));
    }

    #[test]
    fn placeholder_omits_check_for_unverified_artifact() {
        let mut f = ArtifactFilter::with_thread_url("https://app.oxy.tech/threads/T1".to_string());
        let out =
            f.feed(":::artifact{id=a kind=k title=Draft query is_verified=false}\nbody\n:::\n");
        assert!(out.contains("Draft query"));
        assert!(!out.contains(" ✓"));
        assert!(!out.contains("**"), "no bold expected: {out}");
    }

    #[test]
    fn no_placeholder_when_title_missing() {
        let mut f = ArtifactFilter::with_thread_url("https://app.oxy.tech/threads/T1".to_string());
        let out = f.feed(":::artifact{id=a kind=k}\nbody\n:::\n");
        // No title → fall back to drop (nothing surfaced).
        assert_eq!(out, "");
    }

    #[test]
    fn emits_nothing_extra_in_default_mode() {
        let mut f = ArtifactFilter::new();
        let out = f.feed(":::artifact{id=a kind=k title=Foo is_verified=true}\nbody\n:::\n");
        assert_eq!(out, "");
    }
}
