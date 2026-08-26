//! `oxy api` — a `gh api`-style client for the oxy HTTP API.
//!
//! Makes authenticated requests from the terminal using the token cached by
//! `oxy login` (or `OXY_TOKEN`), so you never hand-manage an `Authorization`
//! / `X-API-Key` header while vibe-coding. The path is taken relative to the
//! target's `/api/` surface.
//!
//! The command is deliberately **self-describing**, because the usual caller
//! has the binary and nothing else — no checkout, no running server, no doc
//! site. So everything needed to make a request against the `/api` and
//! `/external/api` surfaces is reachable from `oxy api` itself:
//!
//!   - `--help` — the usage guide plus every route this build mounts there.
//!   - `--routes [FILTER] [--json]` — the same table, narrowed, with what the
//!     server says each route does (from `server::route_catalog`, generated
//!     from the router source at build time).
//!   - `--openapi` — request/response schemas, the same document `oxy serve`
//!     publishes at `/apidoc/openapi.json`.
//!
//! Anything a caller needs *within those surfaces* that is not reachable that
//! way is a bug here, not something to answer by pointing at the source. What
//! is out of scope — the `/customer-apps` bundle tree, the worker health port,
//! the internal loopback router — is listed in `server::route_catalog`.
//!
//! Examples:
//!   oxy api --routes threads                 # discover endpoints
//!   oxy api user --env local
//!   oxy api projects/<id>/query --env local -f sql='select 1'
//!   oxy api admin/compiles/run -X POST -F workspace_id=<id> -F promote=true --env dev
//!   oxy api --print-token --env local        # echo the bearer for raw curl
//!
//! `-f`/`--field` sends every value as a JSON string; `-F`/`--field-typed`
//! parses the value as JSON (so `promote=true` is a bool, `n=3` a number,
//! `'ids=["a","b"]'` an array) and falls back to a string when the value
//! isn't valid JSON (e.g. a bare UUID). Both assemble into one JSON object;
//! on a key clash the typed `-F` value wins.

use std::io::Read;

use clap::Parser;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use serde_json::{Map, Value};

use crate::server::route_catalog;

use super::app_manifest::{OxyAppManifest, resolve_target};
use super::login;

/// Everything an operator (or an LLM driving the CLI) needs to make a correct
/// request, appended to `--help` ahead of the generated route table.
///
/// Kept here rather than in `apidoc.md` because that file is the Swagger UI
/// header — it describes the API to someone who already has a browser open on
/// a running server, which is exactly the situation `oxy api` exists to avoid.
const HELP_GUIDE: &str = "\
AUTHENTICATION
  `oxy login [--env <env>]` caches a token per host in ~/.config/oxy/credentials.json;
  every later `oxy api` call against that host picks it up. `OXY_TOKEN` (or
  `--token-env <VAR>`) overrides the cache — that is the CI path. The token is sent as
  `Authorization: Bearer <token>`; `oxy api --print-token` echoes it for raw curl.

CHOOSING A TARGET
  --env local        http://localhost:5173   (the Vite dev server, which proxies /api)
  --env dev          https://aip.dev.oxy.tech
  --env staging      https://aip.staging.oxy.tech
  --env production   https://app.oxygen-hq.com   (the default)
  An `oxy-app.json` `environments` map in the current directory overrides these by name,
  an --env that names no environment is read as a URL, and --target <url> wins outright.

PATHS
  The path is relative to the target's `/api/` surface, so `oxy api user` requests
  `/api/user`; a leading `/` or `api/` is accepted and normalised. Reach the API-key-only
  surface by passing its full path (`oxy api /external/api/<workspace_id>/sql/query`).

  `{...}` segments in the route table are placeholders you substitute. To find real ids:
    oxy api orgs                              # -> [{ id, name, ... }]
    oxy api orgs/<org_id>/workspaces          # -> workspaces you can reach
    oxy api <workspace_id>/agents             # workspace-scoped calls take the id first

REQUEST BODIES
  -f key=value   string field    -F key=value   JSON-typed field (true / 3 / [\"a\"])
  -d '<json>'    raw body        -d @file       from a file       -d -   from stdin
  Fields assemble into one JSON object; a typed -F wins a key clash. The method defaults
  to GET, or POST when a body is present; -X overrides it.

OUTPUT
  The response body goes to stdout verbatim (pipe it to `jq`), errors to stderr with a
  non-zero exit. -i also prints the status line and response headers.

DISCOVERY — everything below works offline, with no server and no token
  oxy api --routes                 every endpoint, grouped by surface
  oxy api --routes threads         matching routes, each with what the server says it does
  oxy api --routes threads --json  the same, as JSON (adds fleet role and path parameters)
  oxy api --openapi                the OpenAPI 3.1 spec — request/response schemas for the
                                   documented subset, the same document `oxy serve` puts at
                                   /apidoc. Pipe to jq: `oxy api --openapi | jq .paths`
";

#[derive(Parser, Debug)]
#[command(after_long_help = help_epilogue())]
pub struct ApiArgs {
    /// API path, relative to the target's `/api/` surface. A leading `/` or
    /// `api/` is accepted and normalised. E.g. `user`,
    /// `projects/<id>/query`, `/api/customer-apps/oxy-access`.
    ///
    /// Run `oxy api --routes` (or read the ROUTES section of `--help`) for the
    /// complete list of paths this server mounts.
    #[arg(required_unless_present_any = ["print_token", "routes", "openapi"])]
    path: Option<String>,

    /// HTTP method. Defaults to GET, or POST when a body (`-d`/`-f`) is given.
    #[arg(short = 'X', long)]
    method: Option<String>,

    /// Request body: a raw string, `@file` to read a file, or `-` for stdin.
    #[arg(short = 'd', long, conflicts_with_all = ["field", "field_typed"])]
    data: Option<String>,

    /// String field `key=value`, repeatable; assembled into a JSON object body.
    /// The value is always sent as a JSON string (use `-F` for typed values).
    #[arg(short = 'f', long = "field")]
    field: Vec<String>,

    /// Typed field `key=value`, repeatable; the value is parsed as JSON
    /// (`true`/`123`/`["a","b"]`) and falls back to a string when it isn't
    /// valid JSON (e.g. a bare UUID). Merged with `-f` into one object; on a
    /// key clash the typed value wins.
    #[arg(short = 'F', long = "field-typed")]
    field_typed: Vec<String>,

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
    #[arg(long, conflicts_with = "path")]
    print_token: bool,

    /// List the API routes in this binary's router and exit, optionally
    /// filtered by a substring matched against the method, path or surface.
    /// Needs no network, no credentials and no running server. A few mounts
    /// are mode-conditional, so a listed path can still 404 on a given
    /// deployment.
    #[arg(
        long,
        num_args = 0..=1,
        default_missing_value = "",
        value_name = "FILTER",
        conflicts_with_all = ["path", "openapi", "print_token"],
    )]
    routes: Option<String>,

    /// Emit `--routes` as JSON (one object per endpoint) instead of a table.
    ///
    /// The conflicts are spelled out as well as the `requires`: once `routes`
    /// carries its own `conflicts_with_all`, clap stops enforcing a `requires`
    /// that points at it from a request invocation, and `oxy api user --json`
    /// starts parsing as a silent no-op. Naming both directions keeps every
    /// mode-mixing combination an error.
    #[arg(long, requires = "routes", conflicts_with_all = ["path", "openapi", "print_token"])]
    json: bool,

    /// Print the OpenAPI 3.1 spec and exit — request/response schemas for the
    /// documented subset of the API, offline. Same document `oxy serve` serves
    /// at `/apidoc/openapi.json`.
    #[arg(long, conflicts_with_all = ["path", "print_token"])]
    openapi: bool,
}

/// `--help` epilogue: the usage guide plus the generated route table.
///
/// clap assembles every subcommand's help on every `oxy` invocation — `oxy
/// serve` included — so this is built once and handed out by reference rather
/// than re-concatenated 600 routes at a time per process.
fn help_epilogue() -> &'static str {
    static EPILOGUE: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        format!(
            "{HELP_GUIDE}\nROUTES — {} endpoints, read from this binary's router at build time.\n\
             Covers the /api and /external/api surfaces. A few mounts are conditional\n\
             (`/setup/*` and the git routes only exist in local mode), so a listed path\n\
             can still 404 on a given deployment. Narrow with `oxy api --routes <filter>`\n\
             to also see what each one does.\n{}",
            route_catalog::routes().len(),
            route_catalog::listing()
        )
    });
    &EPILOGUE
}

/// Render `--routes`, honouring `--json`.
///
/// Unfiltered output is the compact grouped table (600+ lines is already a
/// lot); a filter narrows it enough to afford the prose, which is the point of
/// filtering in the first place.
fn print_routes(filter: &str, json: bool) -> Result<(), OxyError> {
    let filter = filter.trim();
    let needle = (!filter.is_empty()).then_some(filter);
    let matches = route_catalog::search(needle);

    if json {
        let described: Vec<_> = matches.into_iter().map(route_catalog::describe).collect();
        let rendered = serde_json::to_string_pretty(&described)
            .map_err(|e| OxyError::RuntimeError(format!("serialize routes: {e}")))?;
        println!("{rendered}");
        return Ok(());
    }

    if matches.is_empty() {
        eprintln!(
            "{}",
            format!("no route matches {filter:?}. Run `oxy api --routes` for the full list.")
                .as_str()
                .warning()
        );
        return Ok(());
    }

    // Unfiltered output reuses the same grouped listing `--help` shows, so the
    // two can never disagree.
    if needle.is_none() {
        print!("{}", route_catalog::listing());
        return Ok(());
    }

    for (surface, label, credential) in route_catalog::surfaces() {
        let group: Vec<_> = matches.iter().filter(|r| r.surface == *surface).collect();
        if group.is_empty() {
            continue;
        }
        println!("\n{label} — {credential}");
        for r in group {
            println!("  {:<7} {}", r.method, r.path);
            for line in wrap(r.description, 86) {
                println!("          {line}");
            }
            // Marked, because a mount comment is not always *about* its mount —
            // it may be explaining the route above it, or one that was removed.
            for (i, line) in wrap(r.note, 80).iter().enumerate() {
                let lead = if i == 0 { "note: " } else { "      " };
                println!("          {lead}{line}");
            }
        }
    }
    Ok(())
}

/// Greedy word wrap. Route prose is one long line by construction, and a
/// terminal-width paragraph reads better than a 500-column one.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Print the OpenAPI document without starting a server.
async fn print_openapi() -> Result<(), OxyError> {
    let doc = crate::server::router::build_openapi_doc().await;
    let rendered = serde_json::to_string_pretty(&doc)
        .map_err(|e| OxyError::RuntimeError(format!("serialize openapi: {e}")))?;
    println!("{rendered}");
    Ok(())
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

/// Split a `key=value` field flag, erroring with the flag name on a missing `=`.
fn split_field<'a>(flag: &str, f: &'a str) -> Result<(&'a str, &'a str), OxyError> {
    f.split_once('=')
        .ok_or_else(|| OxyError::RuntimeError(format!("invalid {flag} '{f}', expected key=value")))
}

/// Parse a typed `-F` value: try JSON, falling back to a plain string when the
/// value isn't valid JSON (so a bare UUID stays a string, `true` becomes a
/// bool, `123` a number, `["a","b"]` an array).
fn parse_typed_value(v: &str) -> Value {
    serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.to_string()))
}

/// Build a JSON object body from `-f` (string) and `-F` (typed) field pairs,
/// merged into one map. On a key clash the typed `-F` value wins, so it is
/// applied second.
fn fields_to_json(fields: &[String], typed: &[String]) -> Result<String, OxyError> {
    let map = build_fields_map(fields, typed)?;
    serde_json::to_string(&Value::Object(map))
        .map_err(|e| OxyError::RuntimeError(format!("serialize fields: {e}")))
}

fn build_fields_map(fields: &[String], typed: &[String]) -> Result<Map<String, Value>, OxyError> {
    let mut map = Map::new();
    for f in fields {
        let (k, v) = split_field("--field", f)?;
        map.insert(k.to_string(), Value::String(v.to_string()));
    }
    // Typed fields are applied after string fields so they win on key clash.
    for f in typed {
        let (k, v) = split_field("--field-typed", f)?;
        map.insert(k.to_string(), parse_typed_value(v));
    }
    Ok(map)
}

/// Bearer for `target`: `OXY_TOKEN` (or `token_env`) first, then the
/// `oxy login` cache for that host. Shared by every CLI command that calls the
/// oxy HTTP API, so there is one auth story and one error message.
pub(super) fn resolve_bearer(target: &str, token_env: &str, env: &str) -> Result<String, OxyError> {
    env_var(token_env)
        .or_else(|| login::load_token(target))
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "not authenticated for {target}. Run `oxy login --env {env}` (or set {token_env})."
            ))
        })
}

/// The CLI's HTTP client. One place to keep timeouts (and any future proxy /
/// TLS settings) consistent across commands.
pub(super) fn http_client() -> Result<reqwest::Client, OxyError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| OxyError::RuntimeError(format!("http client init: {e}")))
}

fn resolve_token(args: &ApiArgs, target: &str) -> Result<String, OxyError> {
    resolve_bearer(target, &args.token_env, &args.env)
}

pub async fn handle_api_command(args: ApiArgs) -> Result<(), OxyError> {
    // Discovery reads only what is compiled into this binary: no target to
    // resolve, no token to find, nothing to fail on.
    if let Some(filter) = args.routes.as_deref() {
        return print_routes(filter, args.json);
    }
    if args.openapi {
        return print_openapi().await;
    }

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
        None if !args.field.is_empty() || !args.field_typed.is_empty() => {
            Some(fields_to_json(&args.field, &args.field_typed)?)
        }
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

    let client = http_client()?;

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

    /// A bare `--routes` means "no filter", not "missing value".
    #[test]
    fn bare_routes_filters_nothing() {
        let args = ApiArgs::try_parse_from(["oxy", "--routes"]).expect("--routes needs no path");
        assert_eq!(args.routes.as_deref(), Some(""));
        assert!(args.path.is_none());
    }

    /// `oxy api` has four modes — request, `--print-token`, `--routes` and
    /// `--openapi` — and mixing them has to be an error, not a silent pick.
    ///
    /// Table-driven because the relationships are clap's to enforce and clap
    /// resolves them jointly: adding `conflicts_with_all` to `routes` quietly
    /// stopped `--json`'s `requires = "routes"` from firing, so
    /// `oxy api user --json` began parsing as a no-op. Only checking every
    /// combination catches that class.
    #[test]
    fn modes_do_not_mix() {
        for (args, accepted) in [
            // Request mode.
            (vec!["oxy", "user"], true),
            (vec!["oxy", "--print-token"], true),
            (vec!["oxy"], false),
            (vec!["oxy", "user", "--print-token"], false),
            // Discovery modes stand alone.
            (vec!["oxy", "--routes"], true),
            (vec!["oxy", "--routes", "threads"], true),
            (vec!["oxy", "--routes", "--json"], true),
            (vec!["oxy", "--openapi"], true),
            // Mixing them is rejected rather than silently resolved.
            (vec!["oxy", "user", "--routes"], false),
            (vec!["oxy", "user", "--json"], false),
            (vec!["oxy", "user", "--openapi"], false),
            (vec!["oxy", "--routes", "--openapi"], false),
            (vec!["oxy", "--openapi", "--json"], false),
            (vec!["oxy", "--json"], false),
            // `--print-token` is a fourth mode, and was the gap: `routes` and
            // `openapi` named each other and `path`, but not it, so the
            // dispatch order picked a winner in silence.
            (vec!["oxy", "--print-token", "--routes"], false),
            (vec!["oxy", "--print-token", "--openapi"], false),
            (vec!["oxy", "--print-token", "--json"], false),
        ] {
            let parsed = ApiArgs::try_parse_from(&args);
            assert_eq!(
                parsed.is_ok(),
                accepted,
                "`oxy api {}` should {}",
                args[1..].join(" "),
                if accepted { "parse" } else { "be rejected" }
            );
        }
    }

    /// A value after `--routes` is its filter, never a request path.
    #[test]
    fn routes_takes_its_filter_not_a_path() {
        let args = ApiArgs::try_parse_from(["oxy", "--routes", "threads"]).unwrap();
        assert_eq!(args.routes.as_deref(), Some("threads"));
        assert!(args.path.is_none());
    }

    #[test]
    fn openapi_needs_no_path() {
        let args = ApiArgs::try_parse_from(["oxy", "--openapi"]).expect("--openapi needs no path");
        assert!(args.openapi);
        assert!(args.path.is_none());
    }

    /// `--help` is the discovery surface, so it has to actually carry the
    /// routes — an epilogue that lost the generated table would still render.
    #[test]
    fn help_epilogue_carries_the_route_table() {
        let help = help_epilogue();
        assert!(help.contains("AUTHENTICATION"));
        assert!(help.contains("GET     /api/health"));
        assert!(help.contains("/api/{workspace_id}/threads"));
        assert!(
            help.lines().count() > 400,
            "the --help route table shrank to {} lines",
            help.lines().count()
        );
    }

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
        let out = fields_to_json(&["sql=select 1".to_string(), "k=v".to_string()], &[]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["sql"], "select 1");
        assert_eq!(v["k"], "v");
    }

    #[test]
    fn fields_to_json_rejects_missing_equals() {
        assert!(fields_to_json(&["bad".to_string()], &[]).is_err());
    }

    #[test]
    fn fields_to_json_rejects_missing_equals_in_typed() {
        assert!(fields_to_json(&[], &["bad".to_string()]).is_err());
    }

    #[test]
    fn parse_typed_value_bool() {
        assert_eq!(parse_typed_value("true"), Value::Bool(true));
        assert_eq!(parse_typed_value("false"), Value::Bool(false));
    }

    #[test]
    fn parse_typed_value_number() {
        assert_eq!(parse_typed_value("123"), serde_json::json!(123));
        assert_eq!(parse_typed_value("3"), serde_json::json!(3));
    }

    #[test]
    fn parse_typed_value_uuid_falls_back_to_string() {
        // A bare UUID is not valid JSON — it must stay a string, not error.
        let uuid = "5ce5c011-1234-4abc-9def-0123456789ab";
        assert_eq!(parse_typed_value(uuid), Value::String(uuid.to_string()));
    }

    #[test]
    fn parse_typed_value_json_array() {
        assert_eq!(
            parse_typed_value(r#"["a","b"]"#),
            serde_json::json!(["a", "b"])
        );
    }

    #[test]
    fn fields_to_json_typed_produces_native_types() {
        let out = fields_to_json(
            &[],
            &[
                "promote=true".to_string(),
                "n=3".to_string(),
                "workspace_ids=[\"a\",\"b\"]".to_string(),
            ],
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["promote"], Value::Bool(true));
        assert_eq!(v["n"], serde_json::json!(3));
        assert_eq!(v["workspace_ids"], serde_json::json!(["a", "b"]));
    }

    #[test]
    fn fields_to_json_merges_string_and_typed() {
        // -f sql='select 1' merged with -F promote=true into one object.
        let out =
            fields_to_json(&["sql=select 1".to_string()], &["promote=true".to_string()]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["sql"], Value::String("select 1".to_string()));
        assert_eq!(v["promote"], Value::Bool(true));
    }

    #[test]
    fn fields_to_json_typed_wins_on_key_clash() {
        // Same key from -f (string) and -F (typed): the typed value wins.
        let out =
            fields_to_json(&["promote=true".to_string()], &["promote=true".to_string()]).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["promote"], Value::Bool(true));
    }
}
