//! `oxy publish` — self-contained, CI-free deploy for customer apps.
//!
//! From an app directory an engineer runs `oxy publish --env <env>`. The
//! command reads `oxy-app.json`, builds the bundle per its `build` section
//! (or defaults), resolves the target oxy from its `environments` (or
//! defaults), auto-resolves the project from `<target>/api/apps/<org>/<app>/
//! build-config`, authenticates with the token cached by `oxy login` (or
//! `OXY_TOKEN`), and POSTs the tarball to `<target>/api/customer-apps/publish`.
//!
//! Nothing lives in GitHub: no project id, no target var, no per-app build
//! steps — the manifest + `oxy login` carry it all. `--dir` skips the build
//! and uploads a pre-built directory (escape hatch / CI).

use std::path::{Path, PathBuf};

use clap::Parser;
use flate2::Compression;
use flate2::write::GzEncoder;
use oxy::theme::StyledText;
use oxy_shared::errors::OxyError;
use serde::Deserialize;

use super::app_manifest::{OxyAppManifest, resolve_target};
use super::login;

#[derive(Parser, Debug)]
pub struct PublishArgs {
    /// Environment to publish to (resolves the target oxy from oxy-app.json
    /// `environments`, else a built-in default). E.g. local, dev, production.
    #[arg(long, default_value = "production")]
    env: String,
    /// Explicit oxy base URL; overrides `--env`.
    #[arg(long)]
    target: Option<String>,
    /// Org slug. Default: oxy-app.json `orgSlug`, then OXY_ORG, then the
    /// `<org>` segment of an `apps/<org>/<app>/` working directory.
    #[arg(long)]
    org: Option<String>,
    /// App slug. Default: oxy-app.json `slug`, then OXY_APP, then the
    /// `<app>` segment of an `apps/<org>/<app>/` working directory.
    #[arg(long)]
    app: Option<String>,
    /// Project id (UUID). Default: resolved from the target's build-config
    /// endpoint. Override only for unusual setups. Env: OXY_PROJECT.
    #[arg(long)]
    project: Option<String>,
    /// Name of the env var holding the bearer token. Default: OXY_TOKEN.
    /// Falls back to the `oxy login` cache for the target host.
    #[arg(long = "token-env", default_value = "OXY_TOKEN")]
    token_env: String,
    /// Engineer-facing build version. Default: $GITHUB_SHA, else random.
    #[arg(long)]
    build_id: Option<String>,
    /// Skip the build and publish this pre-built directory as-is. When
    /// omitted, `oxy publish` runs the manifest's build and uploads its
    /// output dir.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Publish straight to the live (published) channel instead of draft.
    #[arg(long)]
    promote: bool,
    /// Optional display name override for the app row.
    #[arg(long)]
    name: Option<String>,
}

/// Infer `(org, app)` from a working dir shaped like `.../apps/<org>/<app>[/...]`.
fn infer_org_app_from_cwd() -> Option<(String, String)> {
    let cwd = std::env::current_dir().ok()?;
    let parts: Vec<String> = cwd
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str().map(str::to_string),
            _ => None,
        })
        .collect();
    let idx = parts.iter().rposition(|p| p == "apps")?;
    let org = parts.get(idx + 1)?.clone();
    let app = parts.get(idx + 2)?.clone();
    Some((org, app))
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.trim().is_empty())
}

/// reqwest client with an overall timeout so a hung oxy doesn't wedge the
/// CLI forever. Bundle uploads get a longer budget than the small GETs.
fn http_client(timeout_secs: u64) -> Result<reqwest::Client, OxyError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| OxyError::RuntimeError(format!("http client init: {e}")))
}

/// gzip-tar `dir`'s contents (files at the archive root).
fn tar_gz_dir(dir: &Path) -> Result<Vec<u8>, OxyError> {
    if !dir.is_dir() {
        return Err(OxyError::ConfigurationError(format!(
            "bundle dir {} does not exist (did the build run?)",
            dir.display()
        )));
    }
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    {
        let mut builder = tar::Builder::new(&mut encoder);
        builder
            .append_dir_all("", dir)
            .map_err(|e| OxyError::RuntimeError(format!("tar {}: {e}", dir.display())))?;
        builder
            .finish()
            .map_err(|e| OxyError::RuntimeError(format!("tar finish: {e}")))?;
    }
    encoder
        .finish()
        .map_err(|e| OxyError::RuntimeError(format!("gzip finish: {e}")))
}

/// Run one build step (`install` / `command`) in `cwd` with the serve base
/// path exported, streaming output to the user's terminal.
fn run_build_step(label: &str, cmd: &str, cwd: &Path, base_path: &str) -> Result<(), OxyError> {
    println!("{}", format!("[{label}] $ {cmd}").tertiary());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .env("OXY_APP_BASE_PATH", base_path)
        .status()
        .map_err(|e| OxyError::RuntimeError(format!("failed to spawn `{cmd}`: {e}")))?;
    if !status.success() {
        return Err(OxyError::RuntimeError(format!(
            "build step `{cmd}` failed ({status})"
        )));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct BuildConfigResp {
    project_id: String,
}

/// Resolve the project id from the target oxy using the app's identity.
/// This is what removes OXY_PROJECT from CI and keeps dev/prod correct.
async fn fetch_project(target: &str, org: &str, app: &str) -> Result<String, OxyError> {
    let url = format!("{target}/api/apps/{org}/{app}/build-config");
    let resp = http_client(30)?
        .get(&url)
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("GET {url}: {e}")))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(OxyError::ConfigurationError(format!(
            "app {org}/{app} is not registered on {target}. Register it in the oxy admin UI (or pass --project <uuid>) first."
        )));
    }
    if !resp.status().is_success() {
        return Err(OxyError::RuntimeError(format!(
            "build-config lookup failed ({}) at {url}",
            resp.status()
        )));
    }
    let cfg: BuildConfigResp = resp
        .json()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("parse build-config: {e}")))?;
    Ok(cfg.project_id)
}

pub async fn handle_publish_command(args: PublishArgs) -> Result<(), OxyError> {
    // Auto-load .env.local then .env so a laptop mirrors any shell exports.
    dotenv::from_filename(".env.local").ok();
    dotenv::dotenv().ok();

    let cwd = std::env::current_dir()
        .map_err(|e| OxyError::RuntimeError(format!("cannot read cwd: {e}")))?;
    let manifest = OxyAppManifest::load_from_dir(&cwd);
    let inferred = infer_org_app_from_cwd();

    // Identity: flag → env → manifest → cwd path.
    let org = args
        .org
        .clone()
        .or_else(|| env_var("OXY_ORG"))
        .or_else(|| manifest.as_ref().and_then(|m| m.org_slug.clone()))
        .or_else(|| inferred.as_ref().map(|(o, _)| o.clone()))
        .ok_or_else(|| {
            OxyError::ConfigurationError(
                "missing org: set oxy-app.json orgSlug, --org, or OXY_ORG".into(),
            )
        })?;
    let app = args
        .app
        .clone()
        .or_else(|| env_var("OXY_APP"))
        .or_else(|| manifest.as_ref().and_then(|m| m.slug.clone()))
        .or_else(|| inferred.as_ref().map(|(_, a)| a.clone()))
        .ok_or_else(|| {
            OxyError::ConfigurationError(
                "missing app: set oxy-app.json slug, --app, or OXY_APP".into(),
            )
        })?;

    // Target oxy: --target → manifest environments → built-in default.
    let target = resolve_target(manifest.as_ref(), Some(&args.env), args.target.as_deref())
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "could not resolve a target for --env {}. Pass --target <url> or add it to oxy-app.json environments.",
                args.env
            ))
        })?;

    // Auth: token env (name configurable) → `oxy login` cache for this host.
    let token = env_var(&args.token_env)
        .or_else(|| login::load_token(&target))
        .ok_or_else(|| {
            OxyError::ConfigurationError(format!(
                "not authenticated for {target}. Run `oxy login --env {}` (or set {}).",
                args.env, args.token_env
            ))
        })?;

    // Bundle: build from the manifest, or take a pre-built --dir.
    let bundle_dir = match &args.dir {
        Some(d) => d.clone(),
        None => {
            let m = manifest.as_ref();
            let base_path = format!("/customer-apps/{org}/{app}/");
            let install = m
                .map(|m| m.build_install())
                .unwrap_or_else(|| "pnpm install".into());
            let command = m
                .map(|m| m.build_command())
                .unwrap_or_else(|| "pnpm build".into());
            let out_dir = m.map(|m| m.build_out_dir()).unwrap_or_else(|| "out".into());
            run_build_step("install", &install, &cwd, &base_path)?;
            run_build_step("build", &command, &cwd, &base_path)?;
            cwd.join(out_dir)
        }
    };

    // Project: --project → OXY_PROJECT → build-config on the target.
    let project = match args.project.clone().or_else(|| env_var("OXY_PROJECT")) {
        Some(p) => p,
        None => fetch_project(&target, &org, &app).await?,
    };

    let build_id = args
        .build_id
        .clone()
        .or_else(|| env_var("GITHUB_SHA"))
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string());

    let tarball = tar_gz_dir(&bundle_dir)?;
    let channel = if args.promote { "published" } else { "draft" };
    println!(
        "{}",
        format!(
            "Publishing {org}/{app} ({} bytes) → {target} [{channel}]",
            tarball.len()
        )
        .text()
    );

    let mut form = reqwest::multipart::Form::new()
        .text("org", org.clone())
        .text("app", app.clone())
        .text("project", project)
        .text("build_id", build_id)
        .text("channel", channel.to_string())
        .part(
            "bundle",
            reqwest::multipart::Part::bytes(tarball).file_name("bundle.tar.gz"),
        );
    if let Some(name) = &args.name {
        form = form.text("name", name.clone());
    }

    let url = format!("{target}/api/customer-apps/publish");
    let resp = http_client(120)?
        .post(&url)
        .bearer_auth(&token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("publish request to {url} failed: {e}")))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(OxyError::RuntimeError(format!(
            "publish rejected ({status}): {body}\nAre you an app-admin? Run `oxy login --env {}` to check.",
            args.env
        )));
    }
    if !status.is_success() {
        return Err(OxyError::RuntimeError(format!(
            "publish failed ({status}): {body}"
        )));
    }
    println!("{}", format!("Published: {body}").success());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tar_gz_dir_errors_on_missing_dir() {
        let res = tar_gz_dir(Path::new("/nonexistent/bundle/dir/xyz"));
        assert!(res.is_err());
    }
}
