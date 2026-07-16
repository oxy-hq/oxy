//! `oxy proxy` — local custom-app dev against a cloud Oxy (design doc
//! `internal-docs/2026-07-16-partner-platform-design.md`).
//!
//! An **outbound** sidecar: it does not serve the app (your dev server does).
//! It listens on `--port` (default **3000**, where a local `oxy serve` would be,
//! so a dev server's default Oxy proxy target already matches), and forwards the
//! Oxy calls your dev server proxies to it — attaching the `oxy login --env`
//! bearer and applying guardrails — to the resolved cloud target.
//!
//! The token lives only in this process and is added server-side as an auth
//! FALLBACK (never returned to the browser): the browser's own cookie/session is
//! forwarded transparently, and auth/login endpoints reach the backend
//! unauthenticated so sign-in works. Authorization is decided by the cloud
//! (including partner delegation via the shipped Cedar policy — see design doc
//! §4); the proxy only forwards.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use oxy_shared::errors::OxyError;
use reqwest::Client;

use super::app_manifest::{OxyAppManifest, resolve_target};
use super::login;

/// Cap on a buffered request body. Requests through the proxy are small (query /
/// semantic-query JSON); large payloads are *responses*, which stream.
const MAX_REQUEST_BODY: usize = 25 * 1024 * 1024;

#[derive(clap::Args, Debug)]
pub struct ProxyArgs {
    /// Env to resolve the cloud target + cached token from (same as
    /// `oxy login --env`). First value wins; defaults to `production`.
    #[arg(long, action = clap::ArgAction::Append, value_delimiter = ',')]
    env: Vec<String>,
    /// Explicit oxy base URL; overrides `--env`.
    #[arg(long)]
    target: Option<String>,
    /// Local port to listen on. Default 3000 — stands in for a local `oxy serve`
    /// so a dev server's default Oxy proxy target already points here.
    #[arg(long, default_value_t = 3000)]
    port: u16,
    /// Forward side-effecting calls (`/fn`, agent runs, procedure runs) instead
    /// of holding them.
    #[arg(long)]
    allow_writes: bool,
    /// Forward tracking events instead of dropping them.
    #[arg(long)]
    allow_events: bool,
    /// Confirm proxying to a production target.
    #[arg(long)]
    yes: bool,
}

struct ProxyState {
    target: String,
    /// Fallback bearer for the custom-app case. `None` when the dev isn't logged
    /// in — then the proxy relies purely on the browser's own session (sign-in).
    token: Option<String>,
    allow_writes: bool,
    allow_events: bool,
    client: Client,
}

/// First non-empty `--env`, defaulting to `production` (which the prod guard
/// then gates behind `--yes`).
fn first_env(envs: &[String]) -> String {
    envs.iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "production".to_string())
}

/// `POST /api/customer-apps/{id}/events` — usage tracking. Dropped by default so
/// local dev never writes to the target's analytics.
fn is_events_path(path: &str) -> bool {
    path.starts_with("/api/customer-apps/") && path.ends_with("/events")
}

/// Auth / session endpoints (`/api/auth/*`, `/api/user`). These must reach the
/// backend UNAUTHENTICATED to establish a session (sign-in), so the proxy never
/// injects the dev bearer on them and never holds them behind `--allow-writes`.
fn is_auth_path(path: &str) -> bool {
    path.starts_with("/api/auth/") || path == "/api/user"
}

/// Whether a request should be HELD (the "side-effecting calls held by default"
/// guarantee). Allowlist, not denylist: any mutating method is held EXCEPT the
/// two POST-but-read data-plane endpoints. GET/HEAD are never mutating; tracking
/// events and auth/login endpoints have their own handling and are excluded here.
fn is_write_path(method: &Method, path: &str) -> bool {
    let mutating = matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    if !mutating || is_events_path(path) || is_auth_path(path) {
        return false;
    }
    // POST-but-read: `query` / `semantic-query` carry their filter in the body.
    let is_read_post =
        *method == Method::POST && (path.ends_with("/query") || path.ends_with("/semantic-query"));
    !is_read_post
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

/// Build outbound headers. Forward the browser's own auth (Cookie / Authorization)
/// transparently so a signed-in session works, plus content-type + accept (JSON
/// bodies + SSE). Inject the dev's bearer only as a FALLBACK — when the request
/// carries no auth of its own AND isn't a login/auth endpoint (which must reach
/// the backend unauthenticated). So the dev token never overrides a real browser
/// session and never breaks sign-in.
fn build_request_headers(incoming: &HeaderMap, token: Option<&str>, path: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    for name in [
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::COOKIE,
        header::AUTHORIZATION,
        // Forward the browser's origin so the backend derives the right base URL
        // (it reads Origin/Referer) — critical for the OAuth redirect_uri to match
        // what the provider issued the code for. Without these, sign-in 401s.
        header::ORIGIN,
        header::REFERER,
    ] {
        if let Some(v) = incoming.get(&name) {
            h.insert(name, v.clone());
        }
    }
    let has_auth = h.contains_key(header::COOKIE) || h.contains_key(header::AUTHORIZATION);
    if !has_auth
        && !is_auth_path(path)
        && let Some(token) = token
        && let Ok(v) = HeaderValue::from_str(&format!("Bearer {token}"))
    {
        h.insert(header::AUTHORIZATION, v);
    }
    h
}

/// Make a cloud backend's Set-Cookie storable by the browser on `localhost`
/// (standard dev-proxy rewrite): strip `Domain=` and `Secure`, and normalize
/// `SameSite=None` → `Lax` (None without Secure is rejected; the app→proxy call
/// is same-origin on localhost, so Lax is correct).
fn rewrite_set_cookie(value: &str) -> String {
    value
        .split(';')
        .map(str::trim)
        .filter(|part| {
            let low = part.to_ascii_lowercase();
            !low.starts_with("domain=") && low != "secure"
        })
        .map(|part| {
            if part.eq_ignore_ascii_case("samesite=none") {
                "SameSite=Lax".to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Catch-all: apply guardrails, then forward to the cloud target with the bearer,
/// streaming the response back (handles SSE + large result sets).
async fn forward(State(state): State<Arc<ProxyState>>, req: Request) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
        .to_string();

    if is_events_path(&path) && !state.allow_events {
        tracing::debug!(%path, "oxy proxy: tracking event dropped (--allow-events to send)");
        return StatusCode::NO_CONTENT.into_response();
    }
    if is_write_path(&method, &path) && !state.allow_writes {
        return (
            StatusCode::CONFLICT,
            "oxy proxy: side-effecting call held. Re-run with --allow-writes to permit /fn, agent, and procedure calls against the target.",
        )
            .into_response();
    }

    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, MAX_REQUEST_BODY).await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("oxy proxy: could not read request body: {e}"),
            )
                .into_response();
        }
    };

    let url = format!("{}{}", state.target, path_and_query);
    tracing::debug!(%method, %path, "oxy proxy → cloud");

    let upstream = state
        .client
        .request(method, &url)
        .headers(build_request_headers(
            &parts.headers,
            state.token.as_deref(),
            &path,
        ))
        .body(body_bytes.to_vec())
        .send()
        .await;
    let upstream = match upstream {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("oxy proxy: upstream error reaching {}: {e}", state.target),
            )
                .into_response();
        }
    };

    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut resp_headers = HeaderMap::new();
    for (name, value) in upstream.headers().iter() {
        if is_hop_by_hop(name.as_str()) {
            continue;
        }
        let Ok(n) = HeaderName::from_bytes(name.as_str().as_bytes()) else {
            continue;
        };
        // Rewrite Set-Cookie so the cloud session cookie is accepted on localhost.
        if n == header::SET_COOKIE {
            if let Ok(s) = value.to_str()
                && let Ok(v) = HeaderValue::from_str(&rewrite_set_cookie(s))
            {
                resp_headers.append(n, v);
            }
            continue;
        }
        if let Ok(v) = HeaderValue::from_bytes(value.as_bytes()) {
            resp_headers.append(n, v);
        }
    }

    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    response
}

pub async fn handle_proxy_command(args: ProxyArgs) -> Result<(), OxyError> {
    let cwd =
        std::env::current_dir().map_err(|e| OxyError::RuntimeError(format!("current dir: {e}")))?;
    let manifest = OxyAppManifest::load_from_dir(&cwd);
    let env = first_env(&args.env);

    let target = resolve_target(manifest.as_ref(), Some(&env), args.target.as_deref())
        .ok_or_else(|| {
            OxyError::RuntimeError(format!(
                "could not resolve a target for --env {env}. Pass --target <url> or add it to oxy-app.json environments."
            ))
        })?;

    // Optional: with a token, the proxy adds it as a fallback bearer for the
    // custom-app case; without one, it forwards the browser's own session so you
    // can sign in through the proxy.
    let token = std::env::var("OXY_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| login::load_token(&target));

    let is_prod =
        matches!(env.as_str(), "production" | "prod") || target.contains("://app.oxygen-hq.com");
    if is_prod && !args.yes {
        return Err(OxyError::RuntimeError(format!(
            "refusing to proxy to PRODUCTION ({target}) without confirmation. Re-run with --yes if that's intended."
        )));
    }

    println!();
    println!("  oxy proxy");
    println!("  → target : {target}  [{}]", env.to_uppercase());
    println!(
        "  → auth   : {}",
        if token.is_some() {
            "oxy login token (fallback); browser session forwarded"
        } else {
            "browser session only (no oxy login token — sign in through the proxy)"
        }
    );
    println!(
        "  → events : {}",
        if args.allow_events {
            "forwarded"
        } else {
            "dropped (--allow-events to send)"
        }
    );
    println!(
        "  → writes : {}",
        if args.allow_writes {
            "allowed"
        } else {
            "held (--allow-writes to permit /fn, agents, procedures)"
        }
    );
    println!();

    let state = Arc::new(ProxyState {
        target,
        token,
        allow_writes: args.allow_writes,
        allow_events: args.allow_events,
        client: Client::new(),
    });

    let app = Router::new().fallback(forward).with_state(state);
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        OxyError::RuntimeError(format!(
            "could not bind 127.0.0.1:{} — is a local oxy server already running there? Use --port. ({e})",
            args.port
        ))
    })?;

    println!(
        "  listening on http://127.0.0.1:{}  (point your dev server's Oxy proxy here)",
        args.port
    );
    println!();

    axum::serve(listener, app)
        .await
        .map_err(|e| OxyError::RuntimeError(format!("proxy server error: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_path_matches() {
        assert!(is_events_path("/api/customer-apps/abc/events"));
        assert!(!is_events_path("/api/projects/abc/query"));
    }

    #[test]
    fn holds_all_mutations_except_allowlisted_reads() {
        let post = Method::POST;
        // Known side-effecting shapes are held...
        assert!(is_write_path(&post, "/customer-apps/o/s/fn/refresh"));
        assert!(is_write_path(&post, "/api/projects/x/agents/y/asks"));
        assert!(is_write_path(&post, "/api/projects/x/procedures/y/runs"));
        // ...and so is ANY other mutating method / route (allowlist, not denylist).
        assert!(is_write_path(&Method::DELETE, "/api/projects/x/resource/y"));
        assert!(is_write_path(&Method::PATCH, "/api/projects/x/members/y"));
        assert!(is_write_path(&post, "/api/projects/x/whatever"));
        // The two POST-but-read data-plane endpoints are NOT held.
        assert!(!is_write_path(&post, "/api/projects/x/query"));
        assert!(!is_write_path(&post, "/api/projects/x/semantic-query"));
        // Reads are never held; events are governed by their own guardrail.
        assert!(!is_write_path(&Method::GET, "/api/projects/x/query"));
        assert!(!is_write_path(&post, "/api/customer-apps/x/events"));
        // Auth/login endpoints are never held (sign-in must reach the backend).
        assert!(!is_write_path(&post, "/api/auth/magic-link/verify"));
        assert!(!is_write_path(&post, "/api/auth/google"));
    }

    #[test]
    fn auth_paths_recognized() {
        assert!(is_auth_path("/api/auth/magic-link/verify"));
        assert!(is_auth_path("/api/user"));
        assert!(!is_auth_path("/api/projects/x/query"));
    }

    #[test]
    fn set_cookie_rewrite_makes_it_localhost_storable() {
        let out = rewrite_set_cookie(
            "oxy_session=abc; Domain=.oxygen-hq.com; Path=/; Secure; HttpOnly; SameSite=None",
        );
        assert!(!out.to_lowercase().contains("domain="));
        assert!(!out.to_lowercase().contains("secure"));
        assert!(!out.to_lowercase().contains("samesite=none"));
        assert!(out.contains("SameSite=Lax"));
        assert!(out.contains("oxy_session=abc"));
        assert!(out.contains("HttpOnly"));
    }

    #[test]
    fn first_env_defaults_and_picks_first() {
        assert_eq!(first_env(&[]), "production");
        assert_eq!(first_env(&["  ".into(), "dev".into()]), "dev");
        assert_eq!(first_env(&["staging".into()]), "staging");
    }
}
