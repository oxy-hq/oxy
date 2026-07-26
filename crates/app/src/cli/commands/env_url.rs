//! `--env` values that are URLs rather than environment names.
//!
//! Every oxy CLI command that talks to a deployment takes `--env <name>`
//! (`local` / `dev` / `staging` / `production`, or a key in `oxy-app.json`
//! `environments`). Names are fine once you've memorised them; the URL in your
//! browser's address bar is what you actually have in front of you. So `--env`
//! ALSO accepts a URL, and this module turns one into a target.
//!
//! Two Oxy host schemes carry an org identity, and both are canonicalised back
//! to the deployment's product host so a pasted URL logs you in once per
//! deployment instead of once per org:
//!
//! * **org subdomain** — `<org-slug>.oxygen-hq.com`
//! * **custom-app subdomain** — `<org>--<app>.customer-apps.oxygen-hq.com`
//!
//! The org slug they carry is returned alongside the target, which is what lets
//! `oxy assume start --org <the URL you're looking at>` work.
//!
//! Anything that doesn't match a known zone is used verbatim as its own target
//! (scheme + host + port), so self-hosted and preview deployments work without
//! this table ever knowing about them.

/// An `--env` value resolved to something a request can be made against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedEnv {
    /// Base URL, no trailing slash, no path.
    pub target: String,
    /// Org slug mined from an org / custom-app subdomain, when the URL carried
    /// one. Never invented — `None` means "the URL didn't say".
    pub org_slug: Option<String>,
}

impl ResolvedEnv {
    pub fn new(target: impl Into<String>, org_slug: Option<String>) -> Self {
        Self {
            target: target.into().trim_end_matches('/').to_string(),
            org_slug,
        }
    }
}

/// A deployment's org-subdomain zone and the product host that serves it.
///
/// Mirrors the server's `OXY_ORG_SUBDOMAIN_ZONE` model (one zone per
/// deployment) — see `server::api::org_host_dispatch`. The CLI can't read that
/// env var, so the well-known deployments are listed here; unknown zones fall
/// through to "use the URL as its own target", which is the honest answer.
const ORG_ZONES: &[(&str, &str)] = &[
    ("oxygen-hq.com", "https://app.oxygen-hq.com"),
    ("dev.oxy.tech", "https://aip.dev.oxy.tech"),
    ("staging.oxy.tech", "https://aip.staging.oxy.tech"),
];

/// Host labels that are infrastructure, never an org slug. Matches the spirit
/// of the server's reserved-label list; a reserved label just means "this is
/// the product host, no org implied".
const RESERVED_LABELS: &[&str] = &[
    "app",
    "aip",
    "www",
    "api",
    "admin",
    "static",
    "assets",
    "cdn",
    "docs",
    "customer-apps",
    "customerapps",
];

/// True when an `--env` value should be read as a URL rather than an
/// environment name. Environment names are bare identifiers (`production`,
/// `my-staging`); a `:`, `/`, or `.` means the user pasted a URL.
///
/// Deliberately permissive on the scheme so `app.oxygen-hq.com` (copied without
/// the `https://`) works too.
pub fn looks_like_url(value: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return false;
    }
    v.contains("://") || v.contains('.') || v.contains('/') || v.contains(':')
}

/// Add a scheme to a bare host so `url::Url` will parse it. Loopback gets
/// `http` (nobody runs TLS on `oxy serve` locally); everything else `https`.
fn with_scheme(value: &str) -> String {
    if value.contains("://") {
        return value.to_string();
    }
    // Host = authority up to the port/path. A bracketed IPv6 literal
    // (`[::1]:3000`) is taken through its closing `]`, since a plain split on
    // ':' would otherwise chop `[::1]` into `[`.
    let host = if value.starts_with('[') {
        match value.split_once(']') {
            Some((inner, _)) => format!("{inner}]"),
            None => value.to_string(),
        }
    } else {
        value.split(['/', ':']).next().unwrap_or(value).to_string()
    };
    let loopback = matches!(
        host.as_str(),
        "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]"
    );
    if loopback {
        format!("http://{value}")
    } else {
        format!("https://{value}")
    }
}

/// `scheme://host[:port]` — the path/query/fragment are dropped, because the
/// URL a user pastes is a *page* (`/orgs/…/threads/…`), not an API base.
/// `--target` stays verbatim for the rare deployment served under a path.
fn base_url(url: &url::Url) -> Option<String> {
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let base = match url.port() {
        Some(p) => format!("{}://{host}:{p}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    };
    Some(base)
}

/// The single label of `host` inside `zone` (`poke-house.oxygen-hq.com` in
/// `oxygen-hq.com` → `poke-house`). A multi-label prefix is rejected — that
/// shape belongs to a different zone, not this one.
fn label_in_zone(host: &str, zone: &str) -> Option<String> {
    let label = host.strip_suffix(&format!(".{zone}"))?;
    if label.is_empty() || label.contains('.') {
        return None;
    }
    Some(label.to_string())
}

/// Org slug carried by a custom-app subdomain: `<org>--<app>.customer-apps.<zone>`.
fn custom_app_org(host: &str, zone: &str) -> Option<String> {
    let label = label_in_zone(host, &format!("customer-apps.{zone}"))?;
    let (org, app) = label.split_once("--")?;
    if org.is_empty() || app.is_empty() {
        return None;
    }
    Some(org.to_string())
}

/// Resolve a pasted URL to a target (+ org slug when the host carried one).
/// Returns `None` only when the value can't be parsed as a URL at all.
pub fn parse_env_url(value: &str) -> Option<ResolvedEnv> {
    let url: url::Url = with_scheme(value.trim()).parse().ok()?;
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let base = base_url(&url)?;

    for (zone, product) in ORG_ZONES {
        // Custom-app subdomain first: its host also ends in the org zone, but
        // its label carries a `--` pair that the org rule would mis-read.
        if let Some(org) = custom_app_org(&host, zone) {
            return Some(ResolvedEnv::new(*product, Some(org)));
        }
        let Some(label) = label_in_zone(&host, zone) else {
            continue;
        };
        if RESERVED_LABELS.contains(&label.as_str()) {
            // The product host itself (`app.oxygen-hq.com`) — a target, no org.
            return Some(ResolvedEnv::new(*product, None));
        }
        return Some(ResolvedEnv::new(*product, Some(label)));
    }

    // Unknown zone: the URL is its own target. Self-hosted, preview, loopback.
    Some(ResolvedEnv::new(base, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(value: &str) -> ResolvedEnv {
        parse_env_url(value).expect("parses")
    }

    #[test]
    fn env_names_are_not_urls() {
        for name in ["production", "prod", "dev", "staging", "local", "my_env"] {
            assert!(!looks_like_url(name), "{name} must stay an env name");
        }
        assert!(!looks_like_url(""));
    }

    #[test]
    fn urls_are_recognised_with_or_without_a_scheme() {
        for v in [
            "https://app.oxygen-hq.com",
            "http://localhost:3000",
            "app.oxygen-hq.com",
            "localhost:3000",
        ] {
            assert!(looks_like_url(v), "{v} must be read as a URL");
        }
    }

    #[test]
    fn product_hosts_resolve_to_themselves_with_no_org() {
        assert_eq!(
            resolve("https://app.oxygen-hq.com"),
            ResolvedEnv::new("https://app.oxygen-hq.com", None)
        );
        assert_eq!(
            resolve("https://aip.staging.oxy.tech"),
            ResolvedEnv::new("https://aip.staging.oxy.tech", None)
        );
        assert_eq!(
            resolve("https://aip.dev.oxy.tech"),
            ResolvedEnv::new("https://aip.dev.oxy.tech", None)
        );
    }

    #[test]
    fn org_subdomain_canonicalises_to_the_product_host_and_yields_the_slug() {
        assert_eq!(
            resolve("https://poke-house.oxygen-hq.com"),
            ResolvedEnv::new("https://app.oxygen-hq.com", Some("poke-house".into()))
        );
        assert_eq!(
            resolve("https://poke-house.staging.oxy.tech"),
            ResolvedEnv::new("https://aip.staging.oxy.tech", Some("poke-house".into()))
        );
    }

    #[test]
    fn custom_app_subdomain_yields_the_org_not_the_app() {
        assert_eq!(
            resolve("https://poke-house--bookkeeping.customer-apps.oxygen-hq.com"),
            ResolvedEnv::new("https://app.oxygen-hq.com", Some("poke-house".into()))
        );
    }

    #[test]
    fn a_pasted_page_url_drops_its_path_and_query() {
        assert_eq!(
            resolve("https://app.oxygen-hq.com/threads/abc?x=1#y"),
            ResolvedEnv::new("https://app.oxygen-hq.com", None)
        );
        assert_eq!(
            resolve("https://poke-house.oxygen-hq.com/apps/sales"),
            ResolvedEnv::new("https://app.oxygen-hq.com", Some("poke-house".into()))
        );
    }

    #[test]
    fn unknown_hosts_are_their_own_target() {
        assert_eq!(
            resolve("https://oxy.acme.internal:8443/ide"),
            ResolvedEnv::new("https://oxy.acme.internal:8443", None)
        );
        // Loopback gets http:// when the scheme is omitted.
        assert_eq!(
            resolve("localhost:3000"),
            ResolvedEnv::new("http://localhost:3000", None)
        );
        assert_eq!(
            resolve("http://127.0.0.1:5173"),
            ResolvedEnv::new("http://127.0.0.1:5173", None)
        );
        // Bracketed IPv6 loopback: the port split must not chop `[::1]`.
        assert_eq!(
            resolve("[::1]:3000"),
            ResolvedEnv::new("http://[::1]:3000", None)
        );
    }

    #[test]
    fn multi_label_prefixes_do_not_become_org_slugs() {
        // `a.b.oxygen-hq.com` is not the org-subdomain shape — it must not be
        // read as an org, and it must not be canonicalised away either.
        assert_eq!(
            resolve("https://a.b.oxygen-hq.com"),
            ResolvedEnv::new("https://a.b.oxygen-hq.com", None)
        );
    }

    #[test]
    fn host_case_and_trailing_dot_are_normalised() {
        assert_eq!(
            resolve("https://Poke-House.OXYGEN-HQ.com."),
            ResolvedEnv::new("https://app.oxygen-hq.com", Some("poke-house".into()))
        );
    }
}
