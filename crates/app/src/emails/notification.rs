//! The email a `ctx.notify` call sends when the app asked for email but did not
//! supply its own body: the notification's title, body and a button to open it.
//!
//! Rendered once, when the notification is sent, and stored with the rest of
//! its content in the org's inbox — so the delivery task sends exactly what was
//! composed, on whichever pod drains it. Handlebars escapes every field, which
//! is the point: the title and body are the app's strings.

use handlebars::Handlebars;
use once_cell::sync::Lazy;
use serde::Serialize;

static TEMPLATE: Lazy<Handlebars<'static>> = Lazy::new(|| {
    let mut hbs = Handlebars::new();
    hbs.set_strict_mode(true);
    hbs.register_template_string("notification", include_str!("notification.hbs"))
        .expect("notification.hbs is valid");
    hbs
});

#[derive(Serialize)]
struct View<'a> {
    app_name: &'a str,
    title: &'a str,
    body: Option<&'a str>,
    url: Option<&'a str>,
}

/// `(html, text)` for a notification with no app-supplied email body.
pub fn render(
    app_name: &str,
    title: &str,
    body: Option<&str>,
    url: Option<&str>,
) -> (String, String) {
    let view = View {
        app_name,
        title,
        body,
        url,
    };
    // The template is static and every field is present, so a render failure
    // is a bug in this file — fall back to the text so the notice still goes.
    let text = text_body(title, body, url);
    let html = TEMPLATE.render("notification", &view).unwrap_or_else(|e| {
        tracing::error!(error = %e, "notification email template failed to render");
        format!("<pre>{}</pre>", handlebars::html_escape(&text))
    });
    (html, text)
}

fn text_body(title: &str, body: Option<&str>, url: Option<&str>) -> String {
    let mut out = title.to_string();
    if let Some(b) = body {
        out.push_str("\n\n");
        out.push_str(b);
    }
    if let Some(u) = url {
        out.push_str("\n\n");
        out.push_str(u);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_strings_are_escaped() {
        let (html, text) = render(
            "Store Ops",
            "<script>alert(1)</script>",
            Some("Clean & restock"),
            Some("https://acme--store.example.com/lists/1?a=1&b=2"),
        );
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("Clean &amp; restock"), "{html}");
        assert!(
            text.starts_with("<script>"),
            "text is plain, not HTML: {text}"
        );
        assert!(text.ends_with("?a=1&b=2"), "{text}");
    }

    #[test]
    fn no_link_means_no_button() {
        let (html, _) = render("Store Ops", "Hello", None, None);
        assert!(!html.contains("Open in"), "{html}");
    }
}
