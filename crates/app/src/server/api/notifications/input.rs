//! `ctx.notify`'s payload, parsed and bounded.
//!
//! Every limit here protects someone other than the app: a title a phone can
//! show, a fan-out a person's inbox can absorb, a link that cannot leave the
//! app. Errors are prefixed `InvalidNotifyPayload:` — the label the failure
//! signal files as the app's own mistake.

use serde_json::Value;
use uuid::Uuid;

use super::links::validate_app_link;

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_BODY_CHARS: usize = 2_000;
pub const MAX_TARGETS: usize = 50;
pub const MAX_KEY_LEN: usize = 256;
pub const MAX_EMAIL_HTML_BYTES: usize = 512 * 1024;

/// Who a notification is for, as the app named them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// One person, by Oxy user id (`ctx.user.id`, `ctx.org.people()[].id`).
    User(Uuid),
    /// Whoever holds a position — by role id or exact name — optionally only
    /// at one location (`ctx.org.places()[].id`).
    Role {
        role: String,
        location: Option<Uuid>,
    },
    /// Every member of an org team (`ctx.user.teams[].id`).
    Team(Uuid),
}

/// The email a call asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailRequest {
    /// No email.
    None,
    /// `email: true` — the platform composes it from the title, body and link.
    Default,
    /// The app's own message. Absent fields fall back to the default's.
    Custom {
        subject: Option<String>,
        html: Option<String>,
        text: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct NotifyInput {
    /// Each target with the JSON it came from, echoed back when it is skipped.
    pub targets: Vec<(Target, Value)>,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub email: EmailRequest,
    pub slack: bool,
    pub idempotency_key: Option<String>,
}

fn bad(why: impl std::fmt::Display) -> String {
    format!("InvalidNotifyPayload: {why}")
}

/// The prefix of the platform's own notice kinds (`oxy.workspace_ready`, …).
/// An app may neither declare nor send one: a person's mute of a platform kind
/// is keyed by the kind alone, and a settings page groups these under Oxygen,
/// so an app kind wearing the prefix would borrow both.
pub const RESERVED_KIND_PREFIX: &str = "oxy.";

/// Whether `kind` is in the platform's reserved namespace.
pub fn is_reserved_kind(kind: &str) -> bool {
    kind.starts_with(RESERVED_KIND_PREFIX)
}

/// The refusal for an app kind in the reserved namespace, shared by
/// `ctx.notify` and publish.
pub fn reserved_kind_refusal(kind: &str) -> String {
    format!(
        "NotifyKindReserved: kind `{kind}` starts with `{RESERVED_KIND_PREFIX}`, which is \
         reserved for Oxygen's own notices; rename it"
    )
}

/// The reserved kinds a function's manifest entry declares under
/// `notify.kinds`, sorted. Publish refuses an entry with any.
pub fn reserved_kinds_declared(function_spec: &Value) -> Vec<String> {
    function_spec
        .pointer("/notify/kinds")
        .and_then(Value::as_object)
        .map(|kinds| {
            kinds
                .keys()
                .filter(|k| is_reserved_kind(k))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// A kind is an identifier the person mutes by: short, lowercase, stable.
pub fn valid_kind(kind: &str) -> bool {
    !kind.is_empty()
        && kind.len() <= 64
        && kind
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | '.'))
}

pub fn parse(v: &Value) -> Result<NotifyInput, String> {
    let obj = v.as_object().ok_or_else(|| bad("expected an object"))?;
    let kind = str_field(obj, "kind")?.ok_or_else(|| bad("`kind` is required"))?;
    if !valid_kind(&kind) {
        return Err(bad(
            "`kind` must be 1–64 lowercase letters, digits, `_`, `-` or `.` — it is what a person mutes",
        ));
    }
    if is_reserved_kind(&kind) {
        return Err(reserved_kind_refusal(&kind));
    }
    let title = str_field(obj, "title")?
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| bad("`title` is required"))?;
    bounded("title", &title, MAX_TITLE_CHARS)?;
    let body = str_field(obj, "body")?.filter(|b| !b.trim().is_empty());
    if let Some(b) = &body {
        bounded("body", b, MAX_BODY_CHARS)?;
    }
    let link = str_field(obj, "link")?;
    if let Some(l) = &link {
        validate_app_link(l).map_err(bad)?;
    }
    let idempotency_key = str_field(obj, "idempotencyKey")?;
    if idempotency_key
        .as_ref()
        .is_some_and(|k| k.is_empty() || k.len() > MAX_KEY_LEN)
    {
        return Err(bad("`idempotencyKey` must be 1–256 characters"));
    }
    Ok(NotifyInput {
        targets: targets(obj.get("to"))?,
        kind,
        title,
        body,
        link,
        email: email(obj.get("email"))?,
        slack: match obj.get("slack") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(bad("`slack` must be a boolean")),
        },
        idempotency_key,
    })
}

fn str_field(obj: &serde_json::Map<String, Value>, name: &str) -> Result<Option<String>, String> {
    match obj.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(bad(format!("`{name}` must be a string"))),
    }
}

fn bounded(name: &str, value: &str, max: usize) -> Result<(), String> {
    if value.chars().count() > max {
        return Err(bad(format!("`{name}` exceeds {max} characters")));
    }
    Ok(())
}

fn targets(to: Option<&Value>) -> Result<Vec<(Target, Value)>, String> {
    let list: Vec<&Value> = match to {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::Object(_)) => vec![v],
        None | Some(Value::Null) => return Err(bad("`to` is required")),
        Some(_) => {
            return Err(bad(
                "`to` must be a target or a list of them: { userId } | { role, location? } | { team }",
            ));
        }
    };
    if list.is_empty() {
        return Err(bad("`to` names nobody"));
    }
    if list.len() > MAX_TARGETS {
        return Err(bad(format!("`to` names more than {MAX_TARGETS} targets")));
    }
    list.into_iter()
        .map(|v| target(v).map(|t| (t, v.clone())))
        .collect()
}

fn uuid_field(obj: &serde_json::Map<String, Value>, name: &str) -> Result<Option<Uuid>, String> {
    str_field(obj, name)?
        .map(|s| Uuid::parse_str(s.trim()).map_err(|_| bad(format!("`{name}` must be a uuid"))))
        .transpose()
}

fn target(v: &Value) -> Result<Target, String> {
    let obj = v.as_object().ok_or_else(|| {
        bad("each target is an object: { userId } | { role, location? } | { team }")
    })?;
    let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
    match keys.as_slice() {
        ["userId"] => Ok(Target::User(uuid_field(obj, "userId")?.expect("present"))),
        ["team"] => Ok(Target::Team(uuid_field(obj, "team")?.expect("present"))),
        _ if obj.contains_key("role") && keys.iter().all(|k| matches!(*k, "role" | "location")) => {
            let role = str_field(obj, "role")?
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .ok_or_else(|| bad("`role` must name a position"))?;
            Ok(Target::Role {
                role,
                location: uuid_field(obj, "location")?,
            })
        }
        _ => Err(bad(format!(
            "unrecognised target {v}: expected {{ userId }} | {{ role, location? }} | {{ team }}"
        ))),
    }
}

fn email(v: Option<&Value>) -> Result<EmailRequest, String> {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(EmailRequest::None),
        Some(Value::Bool(true)) => Ok(EmailRequest::Default),
        Some(Value::Object(obj)) => {
            for k in obj.keys() {
                if !matches!(k.as_str(), "subject" | "html" | "text") {
                    return Err(bad(format!(
                        "`email.{k}` is not a field — a notification's email takes subject, html and \
                         text; recipients come from `to`"
                    )));
                }
            }
            let subject = str_field(obj, "subject")?;
            if let Some(s) = &subject {
                bounded("email.subject", s, MAX_TITLE_CHARS)?;
            }
            let html = str_field(obj, "html")?;
            if html
                .as_ref()
                .is_some_and(|h| h.len() > MAX_EMAIL_HTML_BYTES)
            {
                return Err(bad("`email.html` exceeds 512 KiB"));
            }
            Ok(EmailRequest::Custom {
                subject,
                html,
                text: str_field(obj, "text")?,
            })
        }
        Some(_) => Err(bad("`email` must be true or { subject?, html?, text? }")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal() -> Value {
        json!({ "to": { "userId": Uuid::nil().to_string() }, "kind": "list_overdue", "title": "Late" })
    }

    #[test]
    fn a_minimal_call_parses() {
        let n = parse(&minimal()).unwrap();
        assert_eq!(n.targets[0].0, Target::User(Uuid::nil()));
        assert_eq!(n.email, EmailRequest::None);
        assert!(!n.slack);
    }

    #[test]
    fn every_target_shape_parses_and_a_mixed_one_does_not() {
        let loc = Uuid::new_v4();
        let mut v = minimal();
        v["to"] = json!([
            { "role": "Store Manager", "location": loc.to_string() },
            { "role": "Corporate" },
            { "team": Uuid::nil().to_string() },
        ]);
        let n = parse(&v).unwrap();
        assert_eq!(
            n.targets.iter().map(|t| t.0.clone()).collect::<Vec<_>>(),
            vec![
                Target::Role {
                    role: "Store Manager".into(),
                    location: Some(loc)
                },
                Target::Role {
                    role: "Corporate".into(),
                    location: None
                },
                Target::Team(Uuid::nil()),
            ]
        );
        v["to"] = json!({ "userId": Uuid::nil().to_string(), "team": Uuid::nil().to_string() });
        assert!(
            parse(&v)
                .unwrap_err()
                .starts_with("InvalidNotifyPayload: unrecognised target")
        );
        v["to"] = json!({ "email": "a@b.com" });
        assert!(
            parse(&v).is_err(),
            "an address is not a target — that is ctx.email.send"
        );
    }

    #[test]
    fn the_limits_hold() {
        let mut v = minimal();
        v["title"] = json!("x".repeat(MAX_TITLE_CHARS + 1));
        assert!(parse(&v).unwrap_err().contains("`title` exceeds"));
        let mut v = minimal();
        v["kind"] = json!("List Overdue");
        assert!(parse(&v).unwrap_err().contains("`kind`"));
        let mut v = minimal();
        v["to"] = Value::Array(vec![
            json!({ "userId": Uuid::nil().to_string() });
            MAX_TARGETS + 1
        ]);
        assert!(parse(&v).unwrap_err().contains("more than"));
        let mut v = minimal();
        v["link"] = json!("https://elsewhere.example/");
        assert!(parse(&v).unwrap_err().contains("`link`"));
        let mut v = minimal();
        v["email"] = json!({ "to": "x@y.com" });
        assert!(parse(&v).unwrap_err().contains("`email.to` is not a field"));
    }

    #[test]
    fn the_platforms_kind_prefix_is_refused_with_its_own_label() {
        let mut v = minimal();
        v["kind"] = json!("oxy.workspace_ready");
        let err = parse(&v).unwrap_err();
        assert!(err.starts_with("NotifyKindReserved: "), "{err}");
        // Only the prefix is reserved, not the word.
        v["kind"] = json!("oxygen.tank_low");
        assert!(parse(&v).is_ok());
        v["kind"] = json!("store.oxy.x");
        assert!(parse(&v).is_ok());
    }

    #[test]
    fn a_manifest_entry_declaring_a_platform_kind_is_found() {
        let spec = json!({ "notify": { "kinds": { "oxy.anomaly": {}, "late": {} } } });
        assert_eq!(reserved_kinds_declared(&spec), vec!["oxy.anomaly"]);
        assert!(
            reserved_kinds_declared(&json!({ "notify": { "kinds": { "late": {} } } })).is_empty()
        );
        assert!(reserved_kinds_declared(&json!({})).is_empty());
    }

    #[test]
    fn email_true_asks_for_the_default_message() {
        let mut v = minimal();
        v["email"] = json!(true);
        assert_eq!(parse(&v).unwrap().email, EmailRequest::Default);
        v["email"] = json!({ "subject": "S", "html": "<p>h</p>" });
        assert_eq!(
            parse(&v).unwrap().email,
            EmailRequest::Custom {
                subject: Some("S".into()),
                html: Some("<p>h</p>".into()),
                text: None
            }
        );
    }
}
