//! Reverse-proxy backend for remote-hosted custom apps.
//!
//! When a custom-app row's `source_type = "v0"` (i.e. the bundle is
//! deployed elsewhere — Vercel, Netlify, anywhere reachable over HTTPS),
//! oxy proxies every request transparently. The browser sees the bundle at
//! `<oxy>/customer-apps/<org>/<app>/...`, **same origin as oxy itself**, so
//! the visitor's `oxy_session` cookie attaches to the bundle's client-side
//! SDK calls at `/api/projects/.../query` — those land directly on oxy via
//! the same-origin rule the gate enforces.
//!
//! ## Security boundary (oxy ↔ upstream)
//!
//! **Outbound (oxy → upstream):**
//!   - **Strip** `Cookie` and `Authorization` — oxy's session must never
//!     leak to a third-party host.
//!   - **Strip** any incoming `X-Forwarded-*` / `X-Real-IP` — we set our
//!     own based on the real public host the visitor came in on.
//!   - **Strip** hop-by-hop headers per RFC 7230 §6.1.
//!   - **Add** `X-Oxy-*` identity headers (app id, org slug, project id,
//!     user id, user email) so the upstream Next.js app can do
//!     personalised SSR if it wants. Upstream **MUST** treat these as
//!     untrusted unless it independently verifies they came from oxy
//!     (future: signed JWT or shared secret).
//!
//! **Inbound (upstream → oxy → browser):**
//!   - **Strip** `Set-Cookie` — the upstream must not set cookies on oxy's
//!     domain (session-fixation / namespace-collision risk with
//!     `oxy_session`).
//!   - **Strip** hop-by-hop headers.
//!   - Status code + body stream through verbatim.
//!
//! ## Auth model for the upstream's *server-side* calls back to oxy
//!
//! The upstream cannot use the visitor's cookie (we strip it). For
//! server-side data calls, the engineer mints an app-scoped API key via
//! `POST /api/customer-apps/{id}/api-keys` and sets it as `OXY_API_KEY` in
//! the Vercel project's env. The bundle's server components then send it
//! as a bearer to oxy's `/api/projects/.../query`.

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header, header::HeaderName};
use axum::response::{IntoResponse, Response};
use entity::{apps, organizations};
use reqwest::Client;
use uuid::Uuid;

pub struct ProxyIdentity<'a> {
    pub app: &'a apps::Model,
    pub org: &'a organizations::Model,
    pub user_id: Uuid,
    pub user_email: &'a str,
}

pub async fn proxy(
    upstream_base: &str,
    rest: &str,
    method: Method,
    uri: &Uri,
    headers: &HeaderMap,
    body: Body,
    identity: ProxyIdentity<'_>,
) -> Response {
    let Some(url) = build_upstream_url(upstream_base, rest, uri.query()) else {
        tracing::warn!(
            "custom-app proxy: invalid upstream URL (base={upstream_base:?} rest={rest:?})"
        );
        return (StatusCode::BAD_GATEWAY, "invalid upstream URL").into_response();
    };

    let reqwest_method = match reqwest::Method::from_bytes(method.as_str().as_bytes()) {
        Ok(m) => m,
        Err(_) => return (StatusCode::METHOD_NOT_ALLOWED, "bad method").into_response(),
    };

    let out_headers = build_outbound_headers(headers, &identity);

    // Collect the request body into bytes — fine for v1: custom apps are
    // pages + assets + small POSTs. Streaming uploads (multi-MB) would need
    // reqwest::Body::wrap_stream behind the `stream` feature; defer.
    let body_bytes = match axum::body::to_bytes(body, REQUEST_BODY_LIMIT).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("custom-app proxy: failed to buffer request body: {e}");
            return (StatusCode::BAD_REQUEST, "request body too large").into_response();
        }
    };

    let upstream = client()
        .request(reqwest_method, url.clone())
        .headers(out_headers)
        .body(body_bytes)
        .send()
        .await;

    let upstream = match upstream {
        Ok(r) => r,
        Err(err) => {
            tracing::warn!(
                "custom-app proxy: upstream call failed (app={} url={url} err={err})",
                identity.app.id,
            );
            return (
                StatusCode::BAD_GATEWAY,
                "upstream custom-app deploy is unreachable",
            )
                .into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    let mut response_headers = HeaderMap::new();
    for (name, value) in upstream.headers().iter() {
        let Ok(http_name) = HeaderName::from_bytes(name.as_str().as_bytes()) else {
            continue;
        };
        // `HeaderName::as_str()` always returns lower-case ASCII (both
        // axum/http and reqwest enforce this on construction), so the
        // matches in `is_inbound_drop` / `is_outbound_drop` can lower-case
        // their literals. Adding e.g. `"Set-Cookie"` would silently never
        // match.
        if is_inbound_drop(http_name.as_str()) {
            continue;
        }
        if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
            response_headers.append(http_name, v);
        }
    }

    // Stream the response body through to the visitor.
    let stream = upstream.bytes_stream();
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    response
}

/// 32 MiB request-body ceiling. Customer-app POSTs are normally tiny;
/// anything larger is almost certainly user-uploaded media that should
/// hit S3 directly, not be proxied through oxy.
pub(crate) const REQUEST_BODY_LIMIT: usize = 32 * 1024 * 1024;

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // `connect_timeout` only bounds DNS + TCP + TLS — once the upstream
        // is sending data we hold the stream open indefinitely. A blanket
        // `timeout(...)` would cut long-lived SSE / RSC streams off at the
        // total budget regardless of progress.
        //
        // `redirect::Policy::none()` — DO NOT follow upstream redirects on
        // the server side. `is_safe_upstream` only runs on the originally
        // configured URL; an upstream that responds `Location: http://
        // 169.254.169.254/...` would otherwise have the proxy fetch the
        // metadata service with the visitor's `X-Oxy-*` identity attached.
        // Streaming the 3xx straight to the browser is also more
        // transparent — the browser follows the redirect in its own
        // security context (cross-origin redirects fail naturally).
        Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::none())
            // `is_safe_upstream` only inspects the configured URL string, so a
            // public hostname whose A record resolves to `169.254.169.254`/`10.x`/
            // `127.x` (DNS rebinding) would otherwise sail through. Validate every
            // resolved IP at connect time — the same second-layer defense the
            // `ctx.fetch` sibling uses (`custom_apps_functions/host.rs`).
            .dns_resolver(Arc::new(PublicOnlyDnsResolver))
            .build()
            .expect("reqwest client init")
    })
}

/// Partition resolved socket addresses into the ones safe to connect to,
/// dropping any that point at a non-public IP. Factored out so the rebinding
/// decision is unit-testable without real DNS. Returns `Err` with a diagnostic
/// when *every* resolved address is filtered out (the rebinding case). Mirrors
/// the sibling in `custom_apps_functions/host.rs` — keep them in sync.
fn keep_public_addrs(host: &str, resolved: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, String> {
    let safe: Vec<SocketAddr> = resolved
        .into_iter()
        .filter(|addr| is_public_ip(&addr.ip()))
        .collect();
    if safe.is_empty() {
        return Err(format!("'{host}' resolves only to non-public addresses"));
    }
    Ok(safe)
}

/// Second-layer SSRF defense for the custom-app proxy: a custom reqwest DNS
/// resolver that drops every resolved address pointing at a non-public IP,
/// closing the DNS-rebinding vector `is_safe_upstream` can't see. Validation
/// happens at resolution (connect) time, so there is no resolve-then-reconnect
/// TOCTOU window.
#[derive(Debug)]
struct PublicOnlyDnsResolver;

impl reqwest::dns::Resolve for PublicOnlyDnsResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            // Port 0: reqwest overrides it with the URL's port; we only care
            // about the resolved IPs here.
            let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0u16))
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?
                .collect();
            let safe = keep_public_addrs(&host, resolved)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;
            let addrs: reqwest::dns::Addrs = Box::new(safe.into_iter());
            Ok(addrs)
        })
    }
}

fn build_upstream_url(
    upstream_base: &str,
    rest: &str,
    query: Option<&str>,
) -> Option<reqwest::Url> {
    let base = upstream_base.trim_end_matches('/');
    let rest = rest.trim_start_matches('/');
    let full = match query.filter(|q| !q.is_empty()) {
        Some(q) => format!("{base}/{rest}?{q}"),
        None => format!("{base}/{rest}"),
    };
    let url = reqwest::Url::parse(&full).ok()?;
    if !is_safe_upstream(&url) {
        return None;
    }
    Some(url)
}

/// SSRF gate on `source_config.url`. App-admins can register arbitrary
/// upstream URLs — without this, an admin (or anyone who compromises an
/// admin) could point a custom app at `http://169.254.169.254/...` (cloud
/// metadata) or `http://10.0.0.5:6379` (internal Redis) and the proxy would
/// helpfully relay the response back, complete with the injected
/// `X-Oxy-*` identity headers. We refuse:
///   - Anything but `https://` — strips `http://`, `file://`, `ftp://`, etc.
///   - IP-literal hosts that aren't routable public addresses
///     (loopback / private / link-local / unspecified / documentation /
///     broadcast / IPv6 ULA).
///   - Hostnames that look obviously internal (`localhost`, `*.internal`,
///     `*.local`, `*.svc`, `*.cluster.local`).
///
/// A name-only blocklist isn't airtight: a public DNS name can still resolve
/// to a private IP. A truly SSRF-proof gate also resolves DNS at request
/// time and re-checks the resolved IPs — tracked as a follow-up; this is
/// the strong-enough baseline.
fn is_safe_upstream(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    // `Url::host_str` returns `"[::1]"` (with brackets) for IPv6 literals;
    // `IpAddr::parse` wants `"::1"` (without). Strip the bracket pair so a
    // raw IPv6 host parses cleanly before falling back to the hostname check.
    let host_for_ip = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = host_for_ip.parse::<std::net::IpAddr>() {
        return is_public_ip(&ip);
    }
    let lower = host.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "localhost" | "ip6-localhost" | "ip6-loopback"
    ) {
        return false;
    }
    const INTERNAL_SUFFIXES: &[&str] = &[
        ".internal",
        ".local",
        ".localdomain",
        ".svc",
        ".cluster.local",
    ];
    if INTERNAL_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
        return false;
    }
    true
}

fn is_public_ip(ip: &std::net::IpAddr) -> bool {
    // Fold an IPv4-mapped IPv6 address (`::ffff:a.b.c.d`) back to IPv4 before
    // classifying, or `::ffff:169.254.169.254` (cloud metadata) / `::ffff:10.x`
    // slip through the v6 arm as "public". Mirrors the sibling in
    // custom_apps_functions/host.rs — keep them in sync.
    let ip = ip.to_canonical();
    if ip.is_loopback() || ip.is_unspecified() {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => {
            !v4.is_private() && !v4.is_link_local() && !v4.is_broadcast() && !v4.is_documentation()
        }
        std::net::IpAddr::V6(v6) => {
            // `is_unique_local` / `is_unicast_link_local` are unstable in
            // stable Rust; hand-check the prefixes per RFC 4291 / RFC 4193.
            let seg = v6.segments()[0];
            let link_local = (seg & 0xffc0) == 0xfe80;
            let unique_local = (seg & 0xfe00) == 0xfc00;
            !link_local && !unique_local
        }
    }
}

fn build_outbound_headers(
    incoming: &HeaderMap,
    identity: &ProxyIdentity<'_>,
) -> reqwest::header::HeaderMap {
    use reqwest::header as r;
    let mut out = r::HeaderMap::new();
    for (name, value) in incoming.iter() {
        if is_outbound_drop(name.as_str()) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            r::HeaderName::from_bytes(name.as_str().as_bytes()),
            r::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            out.append(n, v);
        }
    }
    let public_host = incoming
        .get("x-forwarded-host")
        .or_else(|| incoming.get(header::HOST))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if public_host.is_empty() {
        // Should never happen in practice — axum guarantees `Host` on HTTP/1.1
        // and synthesises one from `:authority` on HTTP/2. Log to make a
        // misconfigured edge easy to spot when the upstream complains about
        // missing absolute URLs.
        tracing::debug!(
            "custom-app proxy: no x-forwarded-host / host on inbound request (app={})",
            identity.app.id
        );
    }
    let public_proto = incoming
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("https");
    set_str(&mut out, "x-forwarded-host", public_host);
    set_str(&mut out, "x-forwarded-proto", public_proto);
    set_str(
        &mut out,
        "x-forwarded-prefix",
        &format!("/customer-apps/{}/{}", identity.org.slug, identity.app.slug),
    );
    set_str(&mut out, "x-oxy-app-id", &identity.app.id.to_string());
    set_str(&mut out, "x-oxy-app-slug", &identity.app.slug);
    set_str(&mut out, "x-oxy-org-id", &identity.org.id.to_string());
    set_str(&mut out, "x-oxy-org-slug", &identity.org.slug);
    set_str(
        &mut out,
        "x-oxy-project-id",
        &identity.app.project_id.to_string(),
    );
    set_str(&mut out, "x-oxy-user-id", &identity.user_id.to_string());
    set_str(&mut out, "x-oxy-user-email", identity.user_email);
    out
}

fn set_str(headers: &mut reqwest::header::HeaderMap, name: &'static str, value: &str) {
    if value.is_empty() {
        return;
    }
    if let Ok(v) = reqwest::header::HeaderValue::from_str(value) {
        headers.insert(reqwest::header::HeaderName::from_static(name), v);
    }
}

/// Outbound headers we strip on the way to the upstream.
///   - `cookie` / `authorization`: oxy auth must never leak to a third party.
///   - `x-forwarded-*` / `x-real-ip`: we re-derive these from the trusted
///     edge. Forwarding the inbound copy would let a client spoof them.
///   - hop-by-hop per RFC 7230 §6.1. Note: the RFC names the header
///     `Trailer` (singular); `trailers` is the value used in
///     `TE: trailers`. Block both spellings — `Trailer` is rare in
///     practice but listing it is free.
fn is_outbound_drop(name: &str) -> bool {
    // Drop any client-supplied `x-oxy-*` identity header before we re-inject the
    // trusted values in `build_outbound_headers`. Without this, a forged inbound
    // `x-oxy-user-email` survives whenever `set_str` early-returns on an empty
    // trusted value (e.g. a caller with an empty `identity.user_email`), leaking
    // the spoofed identity to the upstream / Oxy Function. `HeaderName::as_str()`
    // is always lowercase, so a lowercase prefix match is exhaustive.
    if name.starts_with("x-oxy-") {
        return true;
    }
    matches!(
        name,
        "cookie"
            | "cookie2"
            | "authorization"
            | "host"
            | "x-forwarded-for"
            | "x-forwarded-host"
            | "x-forwarded-proto"
            | "x-forwarded-prefix"
            | "x-real-ip"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Inbound headers we strip on the way back to the browser.
///   - `set-cookie`: upstream must not set cookies on oxy's origin.
///   - `cookie2` / `set-cookie2` (RFC 2965) are defunct in modern browsers
///     but listed for defense in depth — costs nothing to refuse them.
///   - hop-by-hop (both `trailer` and `trailers` spellings — see
///     `is_outbound_drop`).
fn is_inbound_drop(name: &str) -> bool {
    matches!(
        name,
        "set-cookie"
            | "set-cookie2"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_normalises_slashes() {
        let u = build_upstream_url("https://x.vercel.app/", "/page", None).unwrap();
        assert_eq!(u.as_str(), "https://x.vercel.app/page");
    }

    #[test]
    fn upstream_url_carries_query() {
        let u = build_upstream_url("https://x.vercel.app", "a/b", Some("foo=1&bar=2")).unwrap();
        assert_eq!(u.as_str(), "https://x.vercel.app/a/b?foo=1&bar=2");
    }

    #[test]
    fn upstream_url_rejects_non_https_schemes() {
        assert!(build_upstream_url("file:///etc/passwd", "", None).is_none());
        assert!(build_upstream_url("ftp://x.com", "", None).is_none());
        // http:// is also blocked — same-class SSRF risk via plaintext.
        assert!(build_upstream_url("http://x.vercel.app", "", None).is_none());
    }

    #[test]
    fn upstream_url_rejects_metadata_and_internal_ips() {
        // AWS / GCP / Azure instance-metadata service.
        assert!(
            build_upstream_url("https://169.254.169.254/latest/meta-data/", "", None).is_none()
        );
        // RFC1918.
        assert!(build_upstream_url("https://10.0.0.5", "", None).is_none());
        assert!(build_upstream_url("https://192.168.1.1", "", None).is_none());
        assert!(build_upstream_url("https://172.16.0.1", "", None).is_none());
        // Loopback.
        assert!(build_upstream_url("https://127.0.0.1", "", None).is_none());
        // IPv6 loopback + link-local + ULA.
        assert!(build_upstream_url("https://[::1]", "", None).is_none());
        assert!(build_upstream_url("https://[fe80::1]", "", None).is_none());
        assert!(build_upstream_url("https://[fd00::1]", "", None).is_none());
    }

    #[test]
    fn upstream_url_rejects_internal_hostnames() {
        assert!(build_upstream_url("https://localhost", "", None).is_none());
        assert!(build_upstream_url("https://redis.internal", "", None).is_none());
        assert!(build_upstream_url("https://oxy-app.svc.cluster.local", "", None).is_none());
        assert!(build_upstream_url("https://foo.local", "", None).is_none());
    }

    #[test]
    fn upstream_url_allows_public_hosts() {
        assert!(build_upstream_url("https://x.vercel.app", "", None).is_some());
        assert!(build_upstream_url("https://app.example.com", "", None).is_some());
        // Public IPv4 literal — unusual but valid.
        assert!(build_upstream_url("https://8.8.8.8", "", None).is_some());
    }

    #[test]
    fn outbound_drops_cookie_auth_xforwarded() {
        assert!(is_outbound_drop("cookie"));
        assert!(is_outbound_drop("authorization"));
        assert!(is_outbound_drop("x-forwarded-host"));
        assert!(is_outbound_drop("connection"));
        assert!(!is_outbound_drop("user-agent"));
        assert!(!is_outbound_drop("accept"));
    }

    #[test]
    fn outbound_drops_client_supplied_oxy_identity() {
        // Forged inbound identity headers must never survive to the upstream —
        // `build_outbound_headers` re-injects the trusted values afterwards.
        assert!(is_outbound_drop("x-oxy-user-email"));
        assert!(is_outbound_drop("x-oxy-user-id"));
        assert!(is_outbound_drop("x-oxy-org-slug"));
        assert!(is_outbound_drop("x-oxy-anything-new"));
    }

    #[test]
    fn keep_public_addrs_drops_rebinding_targets() {
        let public: SocketAddr = "8.8.8.8:0".parse().unwrap();
        let metadata: SocketAddr = "169.254.169.254:0".parse().unwrap();
        let rfc1918: SocketAddr = "10.0.0.5:0".parse().unwrap();
        let loopback: SocketAddr = "127.0.0.1:0".parse().unwrap();

        // A mix keeps only the public address.
        let kept = keep_public_addrs("evil.example", vec![metadata, public, rfc1918]).unwrap();
        assert_eq!(kept, vec![public]);

        // All-internal resolution is refused outright (the rebinding case).
        assert!(keep_public_addrs("evil.example", vec![metadata, loopback]).is_err());
    }

    #[test]
    fn inbound_drops_set_cookie() {
        assert!(is_inbound_drop("set-cookie"));
        assert!(is_inbound_drop("set-cookie2"));
        assert!(is_inbound_drop("transfer-encoding"));
        assert!(!is_inbound_drop("content-type"));
        assert!(!is_inbound_drop("cache-control"));
    }
}
