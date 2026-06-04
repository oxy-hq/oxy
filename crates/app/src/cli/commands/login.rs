//! `oxy login` / `oxy logout` — browser-based CLI auth for `oxy publish`.
//!
//! Mirrors the `gh`/`fly`/`vercel` loopback pattern: bind an ephemeral
//! `127.0.0.1` port, open the browser to `<target>/cli-auth?port&state`, and
//! capture the session token the web app hands back to the loopback. The
//! token is the existing JWT (no new minting endpoint); the web `/cli-auth`
//! page reads it from the logged-in session and redirects to the loopback.
//!
//! After capturing the token we call `GET /api/user` and report whether the
//! user is an **app-admin** — i.e. whether they can actually `oxy publish` —
//! so they find out at login time, not on a 403 later.
//!
//! Works identically against a local `oxy serve` (loopback both ways) and
//! the cloud. Tokens are cached per target host in `~/.config/oxy/credentials.json`.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::Parser;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use super::app_manifest::{OxyAppManifest, resolve_target};

#[derive(Parser, Debug)]
pub struct LoginArgs {
    /// Environment name to authenticate against (resolves a target URL from
    /// oxy-app.json `environments`, else a built-in default). E.g. local,
    /// dev, production.
    #[arg(long, default_value = "production")]
    env: String,
    /// Explicit oxy base URL; overrides `--env`.
    #[arg(long)]
    target: Option<String>,
}

#[derive(Parser, Debug)]
pub struct LogoutArgs {
    /// Environment name whose cached token to clear.
    #[arg(long, default_value = "production")]
    env: String,
    /// Explicit oxy base URL; overrides `--env`.
    #[arg(long)]
    target: Option<String>,
}

// ── credentials cache ──────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct CredentialStore(HashMap<String, HostCredential>);

#[derive(Debug, Serialize, Deserialize)]
struct HostCredential {
    token: String,
    email: String,
    is_app_admin: bool,
}

fn credentials_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("oxy").join("credentials.json"))
}

/// Cache key for a target: host[:port], so dev/prod/local tokens are
/// stored separately.
fn host_key(target: &str) -> String {
    url::Url::parse(target)
        .ok()
        .and_then(|u| {
            u.host_str().map(|h| match u.port() {
                Some(p) => format!("{h}:{p}"),
                None => h.to_string(),
            })
        })
        .unwrap_or_else(|| target.trim_end_matches('/').to_string())
}

fn read_store() -> CredentialStore {
    credentials_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_store(store: &CredentialStore) -> Result<(), OxyError> {
    let path = credentials_path()
        .ok_or_else(|| OxyError::RuntimeError("no config dir available".into()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| OxyError::RuntimeError(format!("mkdir {}: {e}", parent.display())))?;
    }
    let json = serde_json::to_string_pretty(store)
        .map_err(|e| OxyError::RuntimeError(format!("serialize credentials: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| OxyError::RuntimeError(format!("write {}: {e}", path.display())))?;
    // The file holds bearer tokens — restrict to the owner on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Cached token for `target`, if any. Used by `oxy publish` as the auth
/// fallback when `OXY_TOKEN` isn't set.
pub fn load_token(target: &str) -> Option<String> {
    read_store()
        .0
        .get(&host_key(target))
        .map(|c| c.token.clone())
        .filter(|t| !t.is_empty())
}

// ── login flow ─────────────────────────────────────────────────────────────

pub async fn handle_login_command(args: LoginArgs) -> Result<(), OxyError> {
    let manifest = OxyAppManifest::load_from_dir(&std::env::current_dir().unwrap_or_default());
    let target = resolve_target(manifest.as_ref(), Some(&args.env), args.target.as_deref())
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "could not resolve a target for --env {}. Pass --target <url> or add it to oxy-app.json environments.",
                args.env
            ))
        })?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| OxyError::RuntimeError(format!("could not bind loopback port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| OxyError::RuntimeError(format!("loopback addr: {e}")))?
        .port();
    let state = uuid::Uuid::new_v4().to_string();

    let auth_url = format!("{target}/cli-auth?port={port}&state={state}");
    println!(
        "{}",
        format!("Opening {auth_url} in your browser to log in…").text()
    );
    println!(
        "{}",
        "If it doesn't open automatically, paste that URL into your browser.".tertiary()
    );
    open_browser(&auth_url);

    let token = wait_for_callback(listener, &state).await?;
    let user = fetch_user(&target, &token).await?;

    let mut store = read_store();
    store.0.insert(
        host_key(&target),
        HostCredential {
            token: token.clone(),
            email: user.email.clone(),
            is_app_admin: user.is_app_admin,
        },
    );
    write_store(&store)?;

    println!(
        "{}",
        format!("Logged in as {} ({target}).", user.email).success()
    );
    if user.is_app_admin {
        println!("{}", "Global admin: yes — you can `oxy publish`.".success());
    } else {
        println!(
            "{}",
            "Global admin: no — you can't publish yet. Ask #platform to add you to OXY_GLOBAL_ADMINS."
                .error()
        );
    }
    Ok(())
}

pub async fn handle_logout_command(args: LogoutArgs) -> Result<(), OxyError> {
    let manifest = OxyAppManifest::load_from_dir(&std::env::current_dir().unwrap_or_default());
    let target = resolve_target(manifest.as_ref(), Some(&args.env), args.target.as_deref())
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "could not resolve a target for --env {}",
                args.env
            ))
        })?;
    let mut store = read_store();
    let removed = store.0.remove(&host_key(&target)).is_some();
    write_store(&store)?;
    if removed {
        println!("{}", format!("Logged out of {target}.").success());
    } else {
        println!("{}", format!("No cached credentials for {target}.").text());
    }
    Ok(())
}

/// Best-effort open of the system browser; failures are non-fatal (we
/// already printed the URL).
fn open_browser(url: &str) {
    let (bin, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    let _ = std::process::Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

const SUCCESS_HTML: &str = "<!doctype html><meta charset=utf-8><title>oxy login</title>\
     <body style=\"font-family:system-ui;padding:3rem;text-align:center\">\
     <h2>Logged in to oxy ✓</h2><p>You can close this tab and return to your terminal.</p>";

/// Accept loopback connections until we see a valid `/callback?token&state`
/// (matching our nonce), then return the token. Ignores stray requests
/// (favicon, etc.). 5-minute budget.
async fn wait_for_callback(
    listener: TcpListener,
    expected_state: &str,
) -> Result<String, OxyError> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        let (mut stream, _) = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| {
                OxyError::RuntimeError(
                    "timed out waiting for the browser to complete login (5 min)".into(),
                )
            })?
            .map_err(|e| OxyError::RuntimeError(format!("loopback accept: {e}")))?;

        let request_target = match read_request_target(&mut stream).await {
            Some(t) => t,
            None => {
                respond(&mut stream, 400, "Bad request").await;
                continue;
            }
        };

        let parsed = url::Url::parse(&format!("http://localhost{request_target}")).ok();
        let Some(parsed) = parsed else {
            respond(&mut stream, 404, "Not found").await;
            continue;
        };
        if parsed.path() != "/callback" {
            respond(&mut stream, 404, "Not found").await;
            continue;
        }

        let mut token = None;
        let mut state = None;
        for (k, v) in parsed.query_pairs() {
            match k.as_ref() {
                "token" => token = Some(v.into_owned()),
                "state" => state = Some(v.into_owned()),
                _ => {}
            }
        }
        if state.as_deref() != Some(expected_state) {
            respond(
                &mut stream,
                400,
                "State mismatch — please retry `oxy login`.",
            )
            .await;
            continue;
        }
        match token.filter(|t| !t.is_empty()) {
            Some(tok) => {
                respond_html(&mut stream, SUCCESS_HTML).await;
                return Ok(tok);
            }
            None => {
                respond(&mut stream, 400, "No token in callback.").await;
            }
        }
    }
}

/// Read just the request line and return its target (path+query), e.g.
/// `/callback?token=…&state=…`.
async fn read_request_target(stream: &mut TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).await.ok()?;
    // "GET /callback?... HTTP/1.1"
    line.split_whitespace().nth(1).map(str::to_string)
}

async fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn respond_html(stream: &mut TcpStream, html: &str) {
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

#[derive(Debug, Deserialize)]
struct UserResp {
    email: String,
    is_app_admin: bool,
}

/// `GET /api/user` with the captured bearer. The public endpoint returns
/// `Option<UserInfo>`; a `null` means the token didn't authenticate.
async fn fetch_user(target: &str, token: &str) -> Result<UserResp, OxyError> {
    let url = format!("{}/api/user", target.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| OxyError::RuntimeError(format!("http client init: {e}")))?;
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(OxyError::RuntimeError(format!(
            "login token rejected by {url} ({})",
            resp.status()
        )));
    }
    let body: Option<UserResp> = resp
        .json()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("parse /api/user: {e}")))?;
    body.ok_or_else(|| {
        OxyError::RuntimeError("login token did not resolve to a user (got null)".into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_separates_envs() {
        assert_eq!(host_key("http://localhost:3000"), "localhost:3000");
        assert_eq!(host_key("https://app.oxygen-hq.com"), "app.oxygen-hq.com");
        assert_eq!(
            host_key("https://app-dev.oxygen-hq.com/"),
            "app-dev.oxygen-hq.com"
        );
    }
}
