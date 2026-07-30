//! Custom-app pretty-URL construction — the `/customer-apps/<org>/<app>/`
//! route scheme.
//!
//! Shared by the custom-apps surface (which generates these links) and the
//! admin apps browser (which displays them). It lives here in `oxy-shared`, a
//! lower crate both depend on, so neither feature surface depends on the other
//! — the arrangement a future crate split requires.

/// Build the canonical pretty URL for a custom app: `/customer-apps/<org>/<app>/`.
///
/// Returns a **relative** URL; the client renders it against its own origin.
/// Custom-app bundles share the SPA's domain in the current model — no
/// whitelabelling yet — so no per-host prefix is needed. (When whitelabelling
/// lands, the right surface is per-app config in the DB, not a global env var.)
/// The trailing slash is load-bearing: relative asset paths append to it.
pub fn build_pretty_url(org_slug: &str, app_slug: &str) -> String {
    format!("/customer-apps/{org_slug}/{app_slug}/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_pretty_url_is_relative() {
        assert_eq!(
            build_pretty_url("acme", "analytics"),
            "/customer-apps/acme/analytics/"
        );
    }
}
