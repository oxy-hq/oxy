//! Where tapping a notification lands.
//!
//! An app says *where in the app* (`/lists/42`); the platform says *where the
//! app is*. Three apps each built that second half by hand — a hard-coded URL,
//! an `APP_URL` secret, a template prop nobody passed — and two of the three
//! were broken. So the app's link is stored relative to the app and resolved
//! here, per surface:
//!
//! - the **inbox** is read on whatever host the reader is on, so it gets the
//!   relative path URL, `/customer-apps/<org>/<app>/lists/42`;
//! - **email and Slack** leave the product, so they get an absolute URL: the
//!   environment's own host when the deployment has a custom-apps zone, the
//!   admin host's path URL otherwise.

use oxy_app_core::custom_app_environment::AppEnvironment;
use oxy_app_core::custom_apps_host_dispatch::{admin_base_url, environment_url_for};
use oxy_shared::utils::custom_app_url::build_pretty_url;

/// Longest link accepted from an app.
pub const MAX_LINK_LEN: usize = 2048;

/// Check an app-supplied link: a path inside the app, nothing that could leave
/// it. `Err` is the reason, for the caller's `InvalidNotifyPayload`.
pub fn validate_app_link(link: &str) -> Result<(), &'static str> {
    if link.len() > MAX_LINK_LEN {
        return Err("`link` exceeds 2048 characters");
    }
    if !link.starts_with('/') || link.starts_with("//") || link.starts_with("/\\") {
        return Err("`link` must be a path inside the app, starting with `/` (e.g. \"/lists/42\")");
    }
    if link.chars().any(|c| c.is_control()) {
        return Err("`link` must not contain control characters");
    }
    // The link is appended to the app's own path, so a `..` segment would
    // climb out of it into another app or the product. Browsers read `\` as
    // `/` and `%2e` as `.`, so both count.
    if link.contains('\\') {
        return Err("`link` must not contain `\\`");
    }
    let path = link.split(['?', '#']).next().unwrap_or_default();
    let climbs = path.split('/').any(|seg| {
        let seg = seg.to_ascii_lowercase().replace("%2e", ".");
        seg == "." || seg == ".."
    });
    if climbs {
        return Err("`link` must stay inside the app: no `.` or `..` segments");
    }
    Ok(())
}

/// `link` appended to a base that ends in `/`.
fn join(base: &str, link: Option<&str>) -> String {
    match link.map(|l| l.trim_start_matches('/')) {
        Some(rest) if !rest.is_empty() => format!("{base}{rest}"),
        _ => base.to_string(),
    }
}

/// The link the inbox shows: relative, so it resolves on any host.
pub fn inbox_href(org_slug: &str, app_slug: Option<&str>, link: Option<&str>) -> Option<String> {
    match app_slug {
        Some(app) => Some(join(&build_pretty_url(org_slug, app), link)),
        // A platform notice's link is already a product path.
        None => link.map(str::to_string),
    }
}

/// The link an email or a Slack message carries: absolute, or `None` when this
/// deployment does not know its own address.
pub fn absolute_url(
    org_slug: &str,
    app_slug: Option<&str>,
    environment: &AppEnvironment,
    link: Option<&str>,
) -> Option<String> {
    let Some(app) = app_slug else {
        let base = admin_base_url()?;
        return Some(format!("{base}{}", link.unwrap_or("/")));
    };
    if let Some(host) = environment_url_for(environment, org_slug, app) {
        return Some(join(&host, link));
    }
    // No custom-apps zone: production is still reachable by its path URL on
    // the admin host. Another environment is only reachable by its own host,
    // so it gets no link rather than one that opens production.
    if *environment != AppEnvironment::Production {
        return None;
    }
    let base = admin_base_url()?;
    Some(join(
        &format!("{base}{}", build_pretty_url(org_slug, app)),
        link,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_link_is_a_path_inside_the_app() {
        assert!(validate_app_link("/lists/42?tab=overdue").is_ok());
        assert!(validate_app_link("/").is_ok());
        assert!(
            validate_app_link("/lists/v1.2?next=../x").is_ok(),
            "a query is not a path"
        );
        for bad in [
            "lists/42",
            "https://evil.example/",
            "//evil.example/x",
            "/\\evil.example",
            "javascript:alert(1)",
            "/a\nb",
            "/../../other-org/other-app/x",
            "/lists/../../x",
            "/%2e%2e/x",
            "/lists/%2E./x",
            "/./x",
            "/lists\\..\\x",
        ] {
            assert!(validate_app_link(bad).is_err(), "{bad} must be refused");
        }
        assert!(validate_app_link(&format!("/{}", "a".repeat(MAX_LINK_LEN))).is_err());
    }

    #[test]
    fn the_inbox_gets_the_apps_relative_path_url() {
        assert_eq!(
            inbox_href("acme", Some("store-ops"), Some("/lists/42")).as_deref(),
            Some("/customer-apps/acme/store-ops/lists/42")
        );
        assert_eq!(
            inbox_href("acme", Some("store-ops"), None).as_deref(),
            Some("/customer-apps/acme/store-ops/")
        );
        assert_eq!(
            inbox_href("acme", None, Some("/work/1")).as_deref(),
            Some("/work/1")
        );
    }
}
