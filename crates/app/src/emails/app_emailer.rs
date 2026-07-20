//! App-facing email sending for Oxy Functions (`ctx.email.send`).
//!
//! Sits on top of the SES transport in this module. The **platform** controls
//! the `from` mailbox — a function author may set `replyTo` only, never `from`.
//! Recipients may be arbitrary external addresses, bounded by a per-send
//! recipient cap. In local/dev the email is logged instead of sent.
//!
//! Design: `internal-docs/2026-07-20-customer-app-email-send-design.md`.

use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message as SesMessage};
use serde::Deserialize;

/// Max combined `to` + `cc` + `bcc` recipients per `ctx.email.send` call
/// (Cloudflare's number). Bounds fan-out from a single send.
pub const MAX_RECIPIENTS_PER_SEND: usize = 50;

/// Default platform sender mailbox when `OXY_APP_EMAIL_FROM` is unset. Must be a
/// verified SES identity in the target account.
const DEFAULT_FROM_MAILBOX: &str = "noreply@oxygen-hq.com";

/// `string | string[]` as it arrives from the JS payload.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

/// Author-supplied payload for `ctx.email.send`, deserialized from the JS object
/// the isolate passes through the host op. `from` is captured only so it can be
/// **rejected** — the platform owns the sender address.
#[derive(Debug, Deserialize)]
pub struct EmailSendInput {
    #[serde(default)]
    to: Option<OneOrMany>,
    #[serde(default)]
    cc: Option<OneOrMany>,
    #[serde(default)]
    bcc: Option<OneOrMany>,
    #[serde(rename = "replyTo", default)]
    reply_to: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(rename = "idempotencyKey", default)]
    idempotency_key: Option<String>,
    /// Not an accepted field — captured only to reject it explicitly.
    #[serde(default)]
    from: Option<String>,
}

/// Sends email on behalf of a customer app's function. Platform-controlled
/// sender; SES transport in cloud, `tracing` log in local/dev. Cheap to
/// construct (reads env only); the SES client is built lazily per send.
pub struct AppEmailer {
    /// Friendly display name for the `From` header (the app's name).
    app_name: String,
    /// The verified platform mailbox all app email is sent from.
    from_mailbox: String,
    /// Region for the SES client (`None` → resolved from the AWS env chain).
    aws_region: Option<String>,
    /// When true, log the composed email instead of calling SES (local/dev).
    local_test: bool,
}

impl AppEmailer {
    /// Build from process env + the app's display name.
    ///
    /// - `OXY_APP_EMAIL_FROM` — sender mailbox (default `noreply@oxygen-hq.com`).
    /// - `OXY_APP_EMAIL_REGION` — SES region (else the default AWS env chain).
    ///
    /// Local mode (see `serve_mode`) defaults to the browser live-preview —
    /// never SES on a laptop; cloud sends via SES. `OXY_APP_EMAIL_LOCAL_TEST`
    /// optionally overrides (`0`/`false` → force SES; any other value → force
    /// preview).
    pub fn from_env(app_name: impl Into<String>) -> Self {
        let from_mailbox = std::env::var("OXY_APP_EMAIL_FROM")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_FROM_MAILBOX.to_string());
        let aws_region = std::env::var("OXY_APP_EMAIL_REGION")
            .ok()
            .filter(|s| !s.trim().is_empty());
        // Default to the browser live-preview in local mode — never call SES on
        // a dev laptop; cloud sends for real. The env var is an optional
        // override (`0`/`false` → SES, any other value → preview).
        let local_test = match std::env::var("OXY_APP_EMAIL_LOCAL_TEST") {
            Ok(v) if !v.trim().is_empty() => !matches!(v.trim(), "0" | "false" | "no"),
            _ => crate::server::serve_mode::process_is_local().unwrap_or(false),
        };
        Self {
            app_name: app_name.into(),
            from_mailbox,
            aws_region,
            local_test,
        }
    }

    /// The `From` header: `App Name <mailbox>` (or bare mailbox when the name is
    /// empty). The display name is RFC-5322 safe — control/CR-LF and `<`/`>` are
    /// dropped (header-injection defense), and a name with specials (comma, `@`,
    /// …) is emitted as an escaped quoted-string.
    fn from_header(&self) -> String {
        match display_name(&self.app_name) {
            Some(name) => format!("{name} <{}>", self.from_mailbox),
            None => self.from_mailbox.clone(),
        }
    }

    /// Validate + send. Returns the value `ctx.email.send` resolves to
    /// (`{ "messageId": ... }`), or `Err(message)` surfaced to JS via the
    /// runtime's `__oxyError` envelope. Error messages are prefixed with a
    /// typed label (`SenderRejected`, `TooManyRecipients`, …).
    pub async fn send(&self, input: EmailSendInput) -> Result<serde_json::Value, String> {
        let msg = self.validate(input)?;
        // Local mode (or the `OXY_APP_EMAIL_LOCAL_TEST` override) previews in the
        // browser and never touches SES. In cloud, SES errors PROPAGATE: a
        // transient failure (DispatchFailure, credential rotation) must re-run via
        // the durable queue — never get swallowed behind a fake `messageId`. A
        // cloud dev box that wants the preview sets `OXY_APP_EMAIL_LOCAL_TEST=1`
        // (the SES-config error message says so).
        if self.local_test {
            return Ok(self.preview_local(&msg));
        }
        self.send_ses(&msg).await
    }

    /// Validate the payload into a normalized, ready-to-send message.
    fn validate(&self, input: EmailSendInput) -> Result<ValidatedEmail, String> {
        if input.from.is_some() {
            return Err(
                "InvalidEmailPayload: `from` is not settable — the platform \
                        controls the sender address; use `replyTo` instead"
                    .to_string(),
            );
        }
        let subject = input
            .subject
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or("InvalidEmailPayload: `subject` is required")?
            .to_string();
        let to = input.to.map(OneOrMany::into_vec).unwrap_or_default();
        if to.is_empty() {
            return Err("InvalidEmailPayload: at least one `to` recipient is required".to_string());
        }
        let cc = input.cc.map(OneOrMany::into_vec).unwrap_or_default();
        let bcc = input.bcc.map(OneOrMany::into_vec).unwrap_or_default();
        let total = to.len() + cc.len() + bcc.len();
        if total > MAX_RECIPIENTS_PER_SEND {
            return Err(format!(
                "TooManyRecipients: {total} recipients exceeds the per-send limit of \
                 {MAX_RECIPIENTS_PER_SEND}"
            ));
        }
        if input.html.is_none() && input.text.is_none() {
            return Err("InvalidEmailPayload: provide `html` or `text`".to_string());
        }
        if input
            .idempotency_key
            .as_deref()
            .is_some_and(|k| k.len() > 256)
        {
            return Err("InvalidEmailPayload: `idempotencyKey` exceeds 256 characters".to_string());
        }
        Ok(ValidatedEmail {
            subject,
            to,
            cc,
            bcc,
            reply_to: input.reply_to.filter(|s| !s.is_empty()),
            html: input.html,
            text: input.text,
        })
    }

    /// Dev path: instead of sending, write the composed email into a dedicated
    /// `oxy-email-previews/` subdir of the temp dir (so it doesn't litter temp;
    /// auto-pruned) and open it in the browser so a developer sees the rendered
    /// template with its real data. Returns a synthetic message id. Prefers HTML.
    fn preview_local(&self, msg: &ValidatedEmail) -> serde_json::Value {
        let message_id = format!("local-test-{}", uuid::Uuid::new_v4());
        // Prefer the HTML body for a faithful preview; fall back to text.
        let (contents, ext) = match (&msg.html, &msg.text) {
            (Some(html), _) => (html.clone(), "html"),
            (None, Some(text)) => (text.clone(), "txt"),
            (None, None) => (String::new(), "txt"),
        };
        let dir = preview_dir();
        prune_old_previews(&dir);
        let path = dir.join(format!("email-{message_id}.{ext}"));
        tracing::info!(
            from = %self.from_header(),
            to = ?msg.to,
            cc = ?msg.cc,
            bcc = ?msg.bcc,
            reply_to = ?msg.reply_to,
            subject = %msg.subject,
            %message_id,
            path = %path.display(),
            "email preview (local mode / SES not configured): not sending; opening rendered email"
        );
        match std::fs::write(&path, contents) {
            Ok(()) => super::local_test::open_in_browser(&path),
            Err(e) => tracing::warn!("failed to write email preview to {}: {e}", path.display()),
        }
        serde_json::json!({ "messageId": message_id })
    }

    /// Cloud path: send via SES v2, returning the SES message id.
    async fn send_ses(&self, msg: &ValidatedEmail) -> Result<serde_json::Value, String> {
        let client = ses_client(self.aws_region.as_deref()).await;

        let mut dest = Destination::builder().set_to_addresses(Some(msg.to.clone()));
        if !msg.cc.is_empty() {
            dest = dest.set_cc_addresses(Some(msg.cc.clone()));
        }
        if !msg.bcc.is_empty() {
            dest = dest.set_bcc_addresses(Some(msg.bcc.clone()));
        }

        let content = SesMessage::builder()
            .subject(text_content(&msg.subject)?)
            .body(build_body(msg.html.as_deref(), msg.text.as_deref())?)
            .build();

        let mut req = client
            .send_email()
            .from_email_address(self.from_header())
            .destination(dest.build())
            .content(EmailContent::builder().simple(content).build());
        if let Some(reply_to) = &msg.reply_to {
            req = req.reply_to_addresses(reply_to.clone());
        }

        let out = req
            .send()
            .await
            .map_err(|e| classify_ses_error(error_detail(&e)))?;
        let message_id = out.message_id().unwrap_or_default().to_string();
        Ok(serde_json::json!({ "messageId": message_id }))
    }
}

/// A validated, normalized email ready to hand to SES or the local logger.
#[derive(Debug)]
struct ValidatedEmail {
    subject: String,
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
    reply_to: Option<String>,
    html: Option<String>,
    text: Option<String>,
}

/// Build a UTF-8 SES `Content`.
fn text_content(data: &str) -> Result<Content, String> {
    Content::builder()
        .data(data)
        .charset("UTF-8")
        .build()
        .map_err(|e| format!("InvalidEmailPayload: {e}"))
}

/// Build the SES `Body` from whichever of html/text is present (at least one is
/// guaranteed by validation).
fn build_body(html: Option<&str>, text: Option<&str>) -> Result<Body, String> {
    let mut body = Body::builder();
    if let Some(html) = html {
        body = body.html(text_content(html)?);
    }
    if let Some(text) = text {
        body = body.text(text_content(text)?);
    }
    Ok(body.build())
}

/// Build an RFC-5322 `display-name` from an app name, or `None` if empty after
/// cleaning. Control chars, CR/LF (header injection), and `<`/`>` are dropped; if
/// the remainder contains a char that isn't valid unquoted (comma, `@`, `.`,
/// `:`, …), it's returned as a `\`/`"`-escaped quoted-string.
fn display_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>'))
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    // atext (RFC 5322 §3.2.3) + space are safe unquoted; anything else needs a
    // quoted-string.
    const ATEXT_SYMBOLS: &str = "!#$%&'*+-/=?^_`{|}~";
    let safe_unquoted = cleaned
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || ATEXT_SYMBOLS.contains(c));
    if safe_unquoted {
        Some(cleaned.to_string())
    } else {
        let escaped = cleaned.replace('\\', "\\\\").replace('"', "\\\"");
        Some(format!("\"{escaped}\""))
    }
}

/// Directory for locally-rendered email previews — one dedicated subdir of the
/// system temp dir (so previews never litter temp), created on demand and kept
/// small by `prune_old_previews`.
fn preview_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("oxy-email-previews");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Age at which a stale preview file becomes eligible for cleanup.
const PREVIEW_TTL: std::time::Duration = std::time::Duration::from_secs(3600);

/// Best-effort cleanup: delete preview files older than `PREVIEW_TTL` so the dir
/// stays small. Runs before each write; the just-written file survives until a
/// later send ages it out.
fn prune_old_previews(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for entry in entries.flatten() {
        let aged_out = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|mtime| now.duration_since(mtime).ok())
            .is_some_and(|age| age > PREVIEW_TTL);
        if aged_out {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Process-wide SES client, built once. The client holds a credentials
/// *provider* (not static creds), so it refreshes creds per call — caching it
/// avoids re-resolving the full AWS config (and its IMDS round-trips) on every
/// send, which matters under the per-invocation send cap. Region is a process
/// constant (env), so the first caller's region wins.
static SES_CLIENT: tokio::sync::OnceCell<aws_sdk_sesv2::Client> =
    tokio::sync::OnceCell::const_new();

async fn ses_client(region: Option<&str>) -> &'static aws_sdk_sesv2::Client {
    SES_CLIENT
        .get_or_init(|| async move {
            let mut loader = aws_config::from_env();
            if let Some(r) = region {
                loader = loader.region(aws_config::Region::new(r.to_string()));
            }
            aws_sdk_sesv2::Client::new(&loader.load().await)
        })
        .await
}

/// Flatten an error and its source chain into one string. AWS `SdkError`'s own
/// Display is terse ("service error"); the useful cause — unverified sender,
/// missing credentials, throttling — lives in the source chain.
fn error_detail(e: &impl std::error::Error) -> String {
    let mut out = e.to_string();
    let mut src = e.source();
    while let Some(s) = src {
        out.push_str(": ");
        out.push_str(&s.to_string());
        src = s.source();
    }
    out
}

/// Best-effort mapping of an SES send failure onto the typed error taxonomy,
/// with an actionable hint. (v1: the label rides in the message; distinct JS
/// `error.name` values are a follow-up that needs a per-op `__oxyError` name.)
fn classify_ses_error(msg: String) -> String {
    let lower = msg.to_lowercase();
    if lower.contains("not verified") {
        format!(
            "SenderRejected: {msg} — the `from` must be a verified SES identity. Set \
             OXY_APP_EMAIL_FROM to a verified sender, or OXY_APP_EMAIL_LOCAL_TEST=1 to preview locally."
        )
    } else if lower.contains("security token")
        || lower.contains("credential")
        || lower.contains("dispatch failure")
    {
        format!(
            "EmailSendFailed: {msg} — AWS credentials/region not resolved on the server. \
             Configure the AWS env, or set OXY_APP_EMAIL_LOCAL_TEST=1 to preview locally without SES."
        )
    } else if lower.contains("reject") {
        format!("SenderRejected: {msg}")
    } else if lower.contains("suspend") || lower.contains("paused") || lower.contains("throttl") {
        format!("RateLimitExceeded: {msg}")
    } else if lower.contains("limit") || lower.contains("quota") {
        format!("DailyLimitExceeded: {msg}")
    } else {
        format!("EmailSendFailed: {msg}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emailer() -> AppEmailer {
        AppEmailer {
            app_name: "Acme Reports".to_string(),
            from_mailbox: "noreply@oxygen-hq.com".to_string(),
            aws_region: None,
            local_test: true,
        }
    }

    fn parse(json: serde_json::Value) -> EmailSendInput {
        serde_json::from_value(json).expect("valid input")
    }

    #[test]
    fn from_header_uses_app_display_name() {
        assert_eq!(
            emailer().from_header(),
            "Acme Reports <noreply@oxygen-hq.com>"
        );
    }

    #[test]
    fn from_header_falls_back_to_bare_mailbox() {
        let mut e = emailer();
        e.app_name = "  ".to_string();
        assert_eq!(e.from_header(), "noreply@oxygen-hq.com");
    }

    #[test]
    fn display_name_leaves_plain_names_unquoted() {
        assert_eq!(display_name("Acme Reports").unwrap(), "Acme Reports");
    }

    #[test]
    fn display_name_quotes_specials() {
        // A comma is illegal in an unquoted display-name → quoted-string.
        assert_eq!(display_name("Acme, Inc").unwrap(), "\"Acme, Inc\"");
    }

    #[test]
    fn display_name_drops_injection_and_escapes_quotes() {
        // CR/LF (header injection) and <> are removed; a literal quote is escaped
        // (not stripped) inside the resulting quoted-string.
        let n = display_name("Ac<me>\r\n\"X").unwrap();
        assert!(!n.contains('<') && !n.contains('>') && !n.contains('\r') && !n.contains('\n'));
        assert!(n.starts_with('"') && n.contains("\\\""));
    }

    #[test]
    fn display_name_empty_after_cleaning_is_none() {
        assert!(display_name("  <>  ").is_none());
    }

    #[test]
    fn classify_ses_error_labels_the_cause() {
        assert!(
            classify_ses_error("Email address is not verified".into())
                .starts_with("SenderRejected")
        );
        // Credential/token errors get the "not resolved" config hint.
        assert!(
            classify_ses_error("The security token is expired".into())
                .contains("credentials/region not resolved")
        );
        assert!(classify_ses_error("some opaque failure".into()).starts_with("EmailSendFailed"));
    }

    #[test]
    fn rejects_caller_supplied_from() {
        let err = emailer()
            .validate(parse(serde_json::json!({
                "to": "a@b.com", "subject": "hi", "text": "yo", "from": "spoof@evil.com"
            })))
            .unwrap_err();
        assert!(err.contains("`from` is not settable"), "{err}");
    }

    #[test]
    fn requires_a_recipient() {
        let err = emailer()
            .validate(parse(serde_json::json!({ "subject": "hi", "text": "yo" })))
            .unwrap_err();
        assert!(err.contains("`to` recipient is required"), "{err}");
    }

    #[test]
    fn requires_subject() {
        let err = emailer()
            .validate(parse(serde_json::json!({ "to": "a@b.com", "text": "yo" })))
            .unwrap_err();
        assert!(err.contains("`subject` is required"), "{err}");
    }

    #[test]
    fn requires_a_body() {
        let err = emailer()
            .validate(parse(
                serde_json::json!({ "to": "a@b.com", "subject": "hi" }),
            ))
            .unwrap_err();
        assert!(err.contains("provide `html`"), "{err}");
    }

    #[test]
    fn enforces_recipient_cap() {
        let many: Vec<String> = (0..60).map(|i| format!("u{i}@b.com")).collect();
        let err = emailer()
            .validate(parse(serde_json::json!({
                "to": many, "subject": "hi", "text": "yo"
            })))
            .unwrap_err();
        assert!(err.starts_with("TooManyRecipients"), "{err}");
    }

    #[test]
    fn accepts_string_or_array_recipients() {
        let one = emailer()
            .validate(parse(serde_json::json!({
                "to": "a@b.com", "subject": "hi", "text": "yo"
            })))
            .unwrap();
        assert_eq!(one.to, vec!["a@b.com".to_string()]);
        let many = emailer()
            .validate(parse(serde_json::json!({
                "to": ["a@b.com", "c@d.com"], "cc": "e@f.com", "subject": "hi", "html": "<b>x</b>"
            })))
            .unwrap();
        assert_eq!(many.to.len(), 2);
        assert_eq!(many.cc, vec!["e@f.com".to_string()]);
    }

    #[test]
    fn rejects_overlong_idempotency_key() {
        let err = emailer()
            .validate(parse(serde_json::json!({
                "to": "a@b.com", "subject": "hi", "text": "yo",
                "idempotencyKey": "x".repeat(300)
            })))
            .unwrap_err();
        assert!(err.contains("idempotencyKey"), "{err}");
    }
}
