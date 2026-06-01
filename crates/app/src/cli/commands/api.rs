//! `oxy api` — a `gh api`-style client for the oxy HTTP API.
//!
//! Makes authenticated requests from the terminal using the token cached by
//! `oxy login` (or `OXY_TOKEN`), so you never hand-manage an `Authorization`
//! / `X-API-Key` header while vibe-coding. The path is taken relative to the
//! target's `/api/` surface.
//!
//! Examples:
//!   oxy api user --env local
//!   oxy api projects/<id>/query --env local -f sql='select 1'
//!   oxy api --print-token --env local        # echo the bearer for raw curl

use std::io::Read;

use clap::Parser;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use serde_json::{Map, Value};

use super::app_manifest::{OxyAppManifest, resolve_target};
use super::login;

#[derive(Parser, Debug)]
pub struct ApiArgs {
    /// API path, relative to the target's `/api/` surface. A leading `/` or
    /// `api/` is accepted and normalised. E.g. `user`,
    /// `projects/<id>/query`, `/api/customer-apps/oxy-access`.
    #[arg(required_unless_present = "print_token")]
    path: Option<String>,

    /// HTTP method. Defaults to GET, or POST when a body (`-d`/`-f`) is given.
    #[arg(short = 'X', long)]
    method: Option<String>,

    /// Request body: a raw string, `@file` to read a file, or `-` for stdin.
    #[arg(short = 'd', long, conflicts_with = "field")]
    data: Option<String>,

    /// Typed JSON field `key=value`, repeatable; assembled into a JSON object
    /// body. Values are sent as strings.
    #[arg(short = 'f', long = "field")]
    field: Vec<String>,

    /// Extra header `Name: value`, repeatable.
    #[arg(short = 'H', long = "header")]
    header: Vec<String>,

    /// Environment to target (resolves the base URL from oxy-app.json
    /// `environments`, else a built-in default). E.g. local, dev, production.
    #[arg(long, default_value = "production")]
    env: String,

    /// Explicit oxy base URL; overrides `--env`.
    #[arg(long)]
    target: Option<String>,

    /// Name of the env var holding the bearer token. Default: OXY_TOKEN.
    /// Falls back to the `oxy login` cache for the target host.
    #[arg(long = "token-env", default_value = "OXY_TOKEN")]
    token_env: String,

    /// Include the response status line + headers in the output.
    #[arg(short = 'i', long)]
    include: bool,

    /// Print the resolved bearer token and exit (handy for raw `curl`).
    #[arg(long)]
    print_token: bool,
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// Normalise a user-supplied path to sit under the target's `/api/` surface.
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim().trim_start_matches('/');
    if trimmed == "api" || trimmed.starts_with("api/") {
        format!("/{trimmed}")
    } else {
        format!("/api/{trimmed}")
    }
}

/// Resolve the request body from `--data` (`-` = stdin, `@file` = file, else
/// the literal string).
fn read_data(data: &str) -> Result<String, OxyError> {
    if data == "-" {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| OxyError::RuntimeError(format!("read stdin: {e}")))?;
        Ok(buf)
    } else if let Some(file) = data.strip_prefix('@') {
        std::fs::read_to_string(file)
            .map_err(|e| OxyError::RuntimeError(format!("read {file}: {e}")))
    } else {
        Ok(data.to_string())
    }
}

/// Build a JSON object body from `key=value` `--field` pairs.
fn fields_to_json(fields: &[String]) -> Result<String, OxyError> {
    let mut map = Map::new();
    for f in fields {
        let (k, v) = f.split_once('=').ok_or_else(|| {
            OxyError::RuntimeError(format!("invalid --field '{f}', expected key=value"))
        })?;
        map.insert(k.to_string(), Value::String(v.to_string()));
    }
    serde_json::to_string(&Value::Object(map))
        .map_err(|e| OxyError::RuntimeError(format!("serialize fields: {e}")))
}

fn resolve_token(args: &ApiArgs, target: &str) -> Result<String, OxyError> {
    env_var(&args.token_env)
        .or_else(|| login::load_token(target))
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "not authenticated for {target}. Run `oxy login --env {}` (or set {}).",
                args.env, args.token_env
            ))
        })
}

pub async fn handle_api_command(args: ApiArgs) -> Result<(), OxyError> {
    // Mirror `oxy publish`: pick up a laptop's shell exports.
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();

    let cwd = std::env::current_dir().unwrap_or_default();
    let manifest = OxyAppManifest::load_from_dir(&cwd);
    let target = resolve_target(manifest.as_ref(), Some(&args.env), args.target.as_deref())
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "could not resolve a target for --env {}. Pass --target <url> or add it to oxy-app.json environments.",
                args.env
            ))
        })?;

    let token = resolve_token(&args, &target)?;
    if args.print_token {
        println!("{token}");
        return Ok(());
    }

    let path = args.path.as_deref().unwrap_or_default();
    let url = format!("{}{}", target.trim_end_matches('/'), normalize_path(path));

    let body = match &args.data {
        Some(d) => Some(read_data(d)?),
        None if !args.field.is_empty() => Some(fields_to_json(&args.field)?),
        None => None,
    };

    let method = args
        .method
        .as_deref()
        .map(str::to_uppercase)
        .unwrap_or_else(|| {
            if body.is_some() {
                "POST".into()
            } else {
                "GET".into()
            }
        });
    let method = reqwest::Method::from_bytes(method.as_bytes())
        .map_err(|e| OxyError::RuntimeError(format!("invalid method: {e}")))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| OxyError::RuntimeError(format!("http client init: {e}")))?;

    let mut req = client.request(method, &url).bearer_auth(&token);
    // Default to JSON when sending a body; a `-H content-type:` overrides it.
    if body.is_some() {
        req = req.header(reqwest::header::CONTENT_TYPE, "application/json");
    }
    for h in &args.header {
        let (name, value) = h.split_once(':').ok_or_else(|| {
            OxyError::RuntimeError(format!("invalid --header '{h}', expected 'Name: value'"))
        })?;
        req = req.header(name.trim(), value.trim());
    }
    if let Some(b) = body {
        req = req.body(b);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("request to {url} failed: {e}")))?;

    let status = resp.status();
    if args.include {
        println!("{:?} {}", resp.version(), status);
        for (name, value) in resp.headers() {
            println!("{name}: {}", value.to_str().unwrap_or("<binary>"));
        }
        println!();
    }
    let text = resp
        .text()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("read response body: {e}")))?;

    if status.is_success() {
        println!("{text}");
        Ok(())
    } else {
        eprintln!("{}", text.error());
        Err(OxyError::RuntimeError(format!(
            "request failed ({})",
            status.as_u16()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_adds_api_prefix() {
        assert_eq!(normalize_path("user"), "/api/user");
        assert_eq!(normalize_path("/user"), "/api/user");
        assert_eq!(normalize_path("projects/x/query"), "/api/projects/x/query");
    }

    #[test]
    fn normalize_path_keeps_existing_api_prefix() {
        assert_eq!(normalize_path("/api/user"), "/api/user");
        assert_eq!(
            normalize_path("api/customer-apps/oxy-access"),
            "/api/customer-apps/oxy-access"
        );
    }

    #[test]
    fn fields_to_json_builds_object() {
        let out = fields_to_json(&["sql=select 1".to_string(), "k=v".to_string()]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["sql"], "select 1");
        assert_eq!(v["k"], "v");
    }

    #[test]
    fn fields_to_json_rejects_missing_equals() {
        assert!(fields_to_json(&["bad".to_string()]).is_err());
    }
}
