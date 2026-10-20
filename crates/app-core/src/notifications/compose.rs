//! The email a `ctx.notify` call asked for, composed when the call is made.
//!
//! Composed then, not at delivery, so the task sends exactly this on whichever
//! pod drains it — and stored with the rest of the content in the org's inbox.
//! An app's own subject or body wins; what it leaves out is filled from the
//! notification itself, with a button that opens the app on the environment's
//! own host.

use crate::custom_app_environment::AppEnvironment;

use super::default_email;
use super::input::{EmailRequest, NotifyInput};
use super::links::absolute_url;
use super::send::EmailContent;

/// Who the email is from and where its link points.
pub struct Sender<'a> {
    pub app_name: &'a str,
    pub app_slug: &'a str,
    /// `None` when the org could not be read: the email goes without a button.
    pub org_slug: Option<&'a str>,
    pub environment: &'a AppEnvironment,
}

/// The email for `parsed`, or `None` when it asked for none. `title` already
/// carries any environment prefix; `prefix` is that prefix, for an app's own
/// subject.
pub fn email(
    sender: &Sender<'_>,
    parsed: &NotifyInput,
    title: &str,
    prefix: &str,
) -> Option<EmailContent> {
    let (subject, html, text) = match &parsed.email {
        EmailRequest::None => return None,
        EmailRequest::Default => (None, None, None),
        EmailRequest::Custom {
            subject,
            html,
            text,
        } => (subject.clone(), html.clone(), text.clone()),
    };
    let subject = subject
        .map(|s| format!("{prefix}{s}"))
        .unwrap_or_else(|| title.to_string());
    if html.is_some() || text.is_some() {
        return Some(EmailContent {
            subject,
            html,
            text,
        });
    }
    let url = sender.org_slug.and_then(|org| {
        absolute_url(
            org,
            Some(sender.app_slug),
            sender.environment,
            parsed.link.as_deref(),
        )
    });
    let (html, text) = default_email::render(
        sender.app_name,
        title,
        parsed.body.as_deref(),
        url.as_deref(),
    );
    Some(EmailContent {
        subject,
        html: Some(html),
        text: Some(text),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parsed(email: serde_json::Value) -> NotifyInput {
        super::super::input::parse(&json!({
            "to": { "userId": uuid::Uuid::nil().to_string() },
            "kind": "list_overdue",
            "title": "Late",
            "body": "3 open",
            "email": email,
        }))
        .unwrap()
    }

    fn sender() -> Sender<'static> {
        Sender {
            app_name: "Store Ops",
            app_slug: "store-ops",
            org_slug: None,
            environment: &AppEnvironment::Staging,
        }
    }

    #[test]
    fn the_apps_own_body_is_kept_and_only_its_subject_is_prefixed() {
        let e = email(
            &sender(),
            &parsed(json!({ "subject": "Overdue", "text": "see the app" })),
            "[staging] Late",
            "[staging] ",
        )
        .unwrap();
        assert_eq!(e.subject, "[staging] Overdue");
        assert_eq!(e.text.as_deref(), Some("see the app"));
        assert_eq!(
            e.html, None,
            "an app's text-only email is not given a platform body"
        );
    }

    #[test]
    fn email_true_renders_the_platform_message_and_no_email_is_none() {
        let e = email(&sender(), &parsed(json!(true)), "Late", "").unwrap();
        assert_eq!(e.subject, "Late");
        assert!(e.html.as_deref().is_some_and(|h| h.contains("3 open")));
        assert!(email(&sender(), &parsed(json!(null)), "Late", "").is_none());
    }
}
