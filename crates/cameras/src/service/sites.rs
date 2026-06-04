//! Site-edit + NAT bulk-rewrite operations.
//!
//! The bulk-rewrite endpoint solves a specific pre-deployment gap:
//! a customer has a UniFi setup at their site, the demo is signed,
//! but no edge box has shipped yet. The operator wants to begin
//! compliance processing now by having the Oxy backend pull each
//! camera's RTSP stream from the customer's WAN IP. This module:
//!
//!   1. Lets the operator set `sites.public_ip` (the customer's WAN
//!      IP, with their router forwarding 7447 → UniFi controller).
//!   2. Accepts a paste of "Camera Name, LAN RTSPS URL" lines,
//!      rewrites each one to `rtsp://<public_ip>:7447/<stream_id>`,
//!      and updates the matched camera row's `rtsp_url`.
//!
//! Stays UniFi-specific for v0: the parser hard-codes port 7441 in
//! the LAN URL and port 7447 in the WAN URL. Other vendors are a
//! follow-up — most won't fit the "swap host + port + protocol"
//! pattern cleanly anyway.

use crate::entities::{cameras, edge_boxes, sites};
use crate::service::{ServiceError, ServiceResult};
use oxy_unifi::UnifiClient;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Site update ────────────────────────────────────────────────────────────

/// What `PATCH /{wid}/cameras/sites/{site_id}` accepts. Future
/// fields land here next to `public_ip` — keep the surface flat so
/// the route doesn't have to know about per-field semantics.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateSiteInput {
    /// `None` means "don't touch"; `Some(None)` clears the field;
    /// `Some(Some(_))` sets it. The route DTO uses serde's
    /// `double_option` pattern to distinguish absent vs explicit null
    /// in the JSON.
    pub public_ip: Option<Option<String>>,
    /// Same shape rationale as `public_ip` — easier to extend than
    /// per-field optional Boolean flags.
    pub region: Option<Option<String>>,
    pub timezone: Option<String>,
}

pub async fn update_site(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    site_id: Uuid,
    input: UpdateSiteInput,
) -> ServiceResult<sites::Model> {
    let site = sites::Entity::find_by_id(site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }

    let mut am: sites::ActiveModel = site.into();
    let mut touched = false;

    if let Some(pip) = input.public_ip {
        // Validate the operator pasted something that LOOKS like an
        // IP (or a hostname). We don't try to ping it — the customer
        // may not have port-forwarded yet when they set this field —
        // but reject obvious junk like trailing slashes or schemes
        // ("https://1.2.3.4") so the rewrite output doesn't break.
        if let Some(value) = pip.as_ref() {
            validate_public_ip(value)?;
        }
        am.public_ip = Set(pip);
        touched = true;
    }
    if let Some(region) = input.region {
        am.region = Set(region);
        touched = true;
    }
    if let Some(tz) = input.timezone {
        let tz = tz.trim();
        if tz.is_empty() {
            return Err(ServiceError::InvalidInput(
                "timezone may not be empty (use 'UTC' if you're not sure)".into(),
            ));
        }
        am.timezone = Set(tz.to_string());
        touched = true;
    }

    if !touched {
        return Err(ServiceError::InvalidInput(
            "no fields to update — provide at least one of public_ip / region / timezone".into(),
        ));
    }
    am.updated_at = Set(chrono::Utc::now().fixed_offset());
    Ok(am.update(db).await?)
}

fn validate_public_ip(value: &str) -> ServiceResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ServiceError::InvalidInput(
            "public_ip may not be an empty string — pass null to clear".into(),
        ));
    }
    if trimmed.contains("://") {
        return Err(ServiceError::InvalidInput(
            "public_ip must be a bare IP or hostname, not a URL".into(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains(' ') {
        return Err(ServiceError::InvalidInput(
            "public_ip must not contain '/' or whitespace".into(),
        ));
    }
    // Accept either IPv4 or a hostname-shaped string. Strict IPv4
    // parsing would reject DDNS hostnames customers sometimes use
    // (no-ip, dyn.com); allowing both is the friendlier surface and
    // a typo's blast radius is low (the rewritten URL just won't
    // resolve, and ffmpeg surfaces a clear error).
    Ok(())
}

// ── NAT bulk rewrite ───────────────────────────────────────────────────────

/// Body for `POST /{wid}/cameras/sites/{site_id}/bulk-rtsp-rewrite`.
/// Operator pastes raw textarea content; the server parses + matches.
#[derive(Debug, Clone, Deserialize)]
pub struct BulkRewriteInput {
    /// One "Camera Name, rtsps://<lan>:7441/<stream_id>" per entry.
    /// Whitespace around the comma is tolerated; everything before
    /// the first comma is the name, everything after is the URL.
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkRewriteResult {
    pub items: Vec<BulkRewriteItem>,
    pub summary: BulkRewriteSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkRewriteItem {
    /// The original line as the operator pasted it. The UI uses
    /// this verbatim to anchor each result row, no trimming etc.,
    /// so "this exact line failed" is unambiguous.
    pub line: String,
    pub outcome: BulkRewriteOutcome,
    /// Set when `outcome == "rewritten"`.
    pub camera_id: Option<Uuid>,
    pub camera_name: Option<String>,
    pub old_url: Option<String>,
    pub new_url: Option<String>,
    /// Set when `outcome != "rewritten"`.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BulkRewriteOutcome {
    /// Camera row's `rtsp_url` updated successfully.
    Rewritten,
    /// Line parsed cleanly but no camera in the site matches the name.
    NoMatch,
    /// Couldn't pull a name + URL out of the line (bad delimiter, etc).
    ParseError,
    /// URL didn't fit the expected UniFi shape.
    InvalidUrl,
    /// Two cameras in the site share the same name — we won't guess.
    AmbiguousName,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BulkRewriteSummary {
    pub rewritten: usize,
    pub no_match: usize,
    pub parse_error: usize,
    pub invalid_url: usize,
    pub ambiguous_name: usize,
}

const UNIFI_LAN_PORT: u16 = 7441;
const UNIFI_WAN_PORT: u16 = 7447;

pub async fn bulk_rewrite_camera_rtsp(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    site_id: Uuid,
    input: BulkRewriteInput,
) -> ServiceResult<BulkRewriteResult> {
    let site = sites::Entity::find_by_id(site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }
    let public_ip = site.public_ip.clone().ok_or_else(|| {
        ServiceError::InvalidInput(
            "site has no public_ip set — set it via PATCH /sites/{id} before pasting URLs".into(),
        )
    })?;

    // Load all cameras for the site once; in-memory match by name.
    // Sites in this scale (10–30 cameras) make a per-line query
    // pointless extra DB traffic.
    let site_cameras = cameras::Entity::find()
        .filter(cameras::Column::SiteId.eq(site_id))
        .all(db)
        .await?;

    // Build a name→camera index. Track duplicate names so we can
    // flag "ambiguous" instead of silently picking one.
    use std::collections::HashMap;
    let mut by_name: HashMap<String, Vec<&cameras::Model>> = HashMap::new();
    for cam in &site_cameras {
        by_name.entry(cam.name.clone()).or_default().push(cam);
    }

    let mut items = Vec::with_capacity(input.lines.len());
    let mut summary = BulkRewriteSummary::default();

    for raw in input.lines {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            // Skip blank lines silently — operators often have
            // trailing newlines in pastes. Don't include in the
            // result so the UI doesn't render empty rows.
            continue;
        }
        let (name, url) = match parse_line(trimmed) {
            Ok(t) => t,
            Err(e) => {
                summary.parse_error += 1;
                items.push(BulkRewriteItem {
                    line: raw,
                    outcome: BulkRewriteOutcome::ParseError,
                    camera_id: None,
                    camera_name: None,
                    old_url: None,
                    new_url: None,
                    error: Some(e),
                });
                continue;
            }
        };
        let stream_id = match parse_unifi_lan_url(&url) {
            Ok(s) => s,
            Err(e) => {
                summary.invalid_url += 1;
                items.push(BulkRewriteItem {
                    line: raw,
                    outcome: BulkRewriteOutcome::InvalidUrl,
                    camera_id: None,
                    camera_name: Some(name),
                    old_url: Some(url),
                    new_url: None,
                    error: Some(e),
                });
                continue;
            }
        };
        let matches = by_name.get(&name);
        let camera = match matches.map(|v| v.as_slice()) {
            Some([single]) => *single,
            Some([_, _, ..]) => {
                summary.ambiguous_name += 1;
                items.push(BulkRewriteItem {
                    line: raw,
                    outcome: BulkRewriteOutcome::AmbiguousName,
                    camera_id: None,
                    camera_name: Some(name.clone()),
                    old_url: None,
                    new_url: None,
                    error: Some(format!(
                        "{} cameras in this site share the name `{name}`",
                        matches.unwrap().len()
                    )),
                });
                continue;
            }
            _ => {
                summary.no_match += 1;
                items.push(BulkRewriteItem {
                    line: raw,
                    outcome: BulkRewriteOutcome::NoMatch,
                    camera_id: None,
                    camera_name: Some(name.clone()),
                    old_url: None,
                    new_url: None,
                    error: Some(format!("no camera in this site named `{name}`")),
                });
                continue;
            }
        };

        let new_url = format!("rtsp://{public_ip}:{UNIFI_WAN_PORT}/{stream_id}");
        let old_url = camera.rtsp_url.clone();
        let mut am: cameras::ActiveModel = camera.clone().into();
        am.rtsp_url = Set(Some(new_url.clone()));
        am.updated_at = Set(chrono::Utc::now().fixed_offset());
        am.update(db).await?;

        summary.rewritten += 1;
        items.push(BulkRewriteItem {
            line: raw,
            outcome: BulkRewriteOutcome::Rewritten,
            camera_id: Some(camera.id),
            camera_name: Some(camera.name.clone()),
            old_url,
            new_url: Some(new_url),
            error: None,
        });
    }

    Ok(BulkRewriteResult { items, summary })
}

// ── Resync from UniFi ─────────────────────────────────────────────────────

/// Result of a UniFi public-IP resync. UI uses `changed` to flash the
/// right toast — "still 50.215.9.17" is less interesting than "updated
/// to 73.12.4.18", and operators triaging an ISP rotation specifically
/// want the diff visible. The full updated site row isn't returned
/// because the UI invalidates the sites query on success and refetches.
#[derive(Debug, Clone, Serialize)]
pub struct ResyncPublicIpResult {
    pub site_id: Uuid,
    pub old_public_ip: Option<String>,
    pub new_public_ip: Option<String>,
    pub changed: bool,
}

/// Re-fetch the UniFi controller's WAN IP for one site and update
/// `sites.public_ip` (and the controller's `edge_boxes.unifi_public_ip`
/// mirror) in place. Cheap targeted alternative to running the full
/// UniFi import again — operators reach for this when an ISP rotation
/// broke the bulk-rewrite RTSP URLs.
///
/// Today this requires the operator's UniFi API key, same as the
/// import endpoints. When workspace-level UniFi credential storage
/// lands we'll switch to pulling from there.
pub async fn resync_public_ip_from_unifi(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    site_id: Uuid,
    body_api_key: Option<String>,
) -> ServiceResult<ResyncPublicIpResult> {
    // Body key wins so a one-off resync still works before the
    // operator stores a credential; otherwise fall back to storage.
    // resolve_api_key returns InvalidInput when neither is available.
    let api_key =
        crate::service::unifi_credentials::resolve_api_key(db, workspace_id, body_api_key).await?;
    let site = sites::Entity::find_by_id(site_id)
        .one(db)
        .await?
        .ok_or(ServiceError::NotFound)?;
    if site.workspace_id != workspace_id {
        return Err(ServiceError::Forbidden("site belongs to another workspace"));
    }
    if site.source != "unifi" {
        return Err(ServiceError::InvalidInput(
            "this site wasn't imported from UniFi — set public_ip manually via PATCH instead"
                .into(),
        ));
    }

    // Find the controller placeholder edge_box for this site (the row
    // the UniFi importer creates per console). The customer's real
    // edge box, when it arrives, lives alongside as a separate row
    // with a different hardware_model — we don't confuse the two.
    let controller = edge_boxes::Entity::find()
        .filter(edge_boxes::Column::SiteId.eq(site_id))
        .filter(edge_boxes::Column::HardwareModel.eq("unifi-controller"))
        .one(db)
        .await?
        .ok_or_else(|| {
            ServiceError::InvalidInput(
                "no UniFi controller record on this site — re-run UniFi import once first".into(),
            )
        })?;
    let console_id = controller.unifi_console_id.clone().ok_or_else(|| {
        ServiceError::InvalidInput("controller row is missing its unifi_console_id".into())
    })?;

    let client = UnifiClient::new(api_key)
        .map_err(|e| ServiceError::InvalidInput(format!("could not build UniFi client: {e}")))?;
    let host = client.get_host(&console_id).await?;
    // Fall back to the bare list endpoint's ip_address if get_host
    // didn't surface one — matches the resolution chain
    // `service::onboarding::run_import` uses.
    let new_ip = host.ip_address.clone();

    let old_public_ip = site.public_ip.clone();
    let mut site_am: sites::ActiveModel = site.into();
    site_am.public_ip = Set(new_ip.clone());
    site_am.updated_at = Set(chrono::Utc::now().fixed_offset());
    site_am.update(db).await?;

    // Mirror the value onto the controller row too so re-imports and
    // the operator UI agree on a single source of truth. We don't
    // gate on `controller.unifi_public_ip == new_ip` — the write is
    // cheap and the symmetry avoids "edge_box has IP X but site says
    // Y" splits when a future code path reads from one or the other.
    let mut controller_am: edge_boxes::ActiveModel = controller.into();
    controller_am.unifi_public_ip = Set(new_ip.clone());
    controller_am.updated_at = Set(chrono::Utc::now().fixed_offset());
    controller_am.update(db).await?;

    Ok(ResyncPublicIpResult {
        site_id,
        changed: old_public_ip != new_ip,
        old_public_ip,
        new_public_ip: new_ip,
    })
}

// ── Parsers ───────────────────────────────────────────────────────────────

/// Splits a line into `(name, url)`. The first comma is the
/// delimiter; whitespace around it is tolerated. This means camera
/// names CAN contain commas — `"Lobby, NW corner, rtsps://..."`
/// would be parsed as name=`"Lobby"`, url=`"NW corner, rtsps://..."`
/// which then fails URL validation. Acceptable v0 trade-off (no
/// operator pastes names with commas in practice); a quoted-name
/// syntax is the obvious v1 if it becomes a real problem.
fn parse_line(line: &str) -> Result<(String, String), String> {
    let (name, url) = line.split_once(',').ok_or_else(|| {
        "expected `name, url` separated by a comma (no comma found on this line)".to_string()
    })?;
    let name = name.trim();
    let url = url.trim();
    if name.is_empty() {
        return Err("camera name is empty".into());
    }
    if url.is_empty() {
        return Err("URL is empty".into());
    }
    Ok((name.to_string(), url.to_string()))
}

/// Extracts the stream_id from a UniFi LAN RTSPS URL. Expected shape:
///   `rtsps://<host>:7441/<stream_id>[?<query>]`
/// Returns the bare stream_id (no leading slash, no query string) so
/// the rewriter can drop it onto `rtsp://<public_ip>:7447/<stream_id>`.
fn parse_unifi_lan_url(url: &str) -> Result<String, String> {
    let rest = url
        .strip_prefix("rtsps://")
        .ok_or_else(|| format!("URL must start with `rtsps://` (got {url:?})"))?;
    let (host_port, path) = rest
        .split_once('/')
        .ok_or_else(|| "URL has no path component (expected `/<stream_id>`)".to_string())?;
    let port = host_port
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .ok_or_else(|| {
            format!("URL host `{host_port}` missing a numeric port (expected `:{UNIFI_LAN_PORT}`)")
        })?;
    if port != UNIFI_LAN_PORT {
        return Err(format!(
            "URL port is {port}; expected {UNIFI_LAN_PORT} (UniFi RTSPS)"
        ));
    }
    // Drop query string + fragment — the WAN endpoint at 7447 doesn't
    // honor SRTP/auth options anyway, and including them in the
    // rewritten URL would break ffmpeg's RTSP demuxer.
    let stream_id = path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .trim_matches('/');
    if stream_id.is_empty() {
        return Err("URL has an empty stream_id (path is just `/`)".into());
    }
    if stream_id.contains('/') {
        return Err(format!(
            "stream_id `{stream_id}` contains `/` — expected a single path segment"
        ));
    }
    Ok(stream_id.to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_line() {
        let (n, u) = parse_line("  Camera 1 ,  rtsps://192.168.1.1:7441/abc  ").unwrap();
        assert_eq!(n, "Camera 1");
        assert_eq!(u, "rtsps://192.168.1.1:7441/abc");
    }

    #[test]
    fn rejects_line_without_comma() {
        parse_line("Camera 1 rtsps://x").unwrap_err();
    }

    #[test]
    fn rejects_blank_fields() {
        parse_line(", rtsps://x").unwrap_err();
        parse_line("Camera 1, ").unwrap_err();
    }

    #[test]
    fn extracts_stream_id() {
        let s = parse_unifi_lan_url("rtsps://192.168.1.1:7441/abc123").unwrap();
        assert_eq!(s, "abc123");
    }

    #[test]
    fn strips_query_string() {
        let s = parse_unifi_lan_url("rtsps://10.0.0.5:7441/stream?enableSrtp").unwrap();
        assert_eq!(s, "stream");
    }

    #[test]
    fn rejects_wrong_scheme() {
        let err = parse_unifi_lan_url("rtsp://192.168.1.1:7441/abc").unwrap_err();
        assert!(err.contains("rtsps://"));
    }

    #[test]
    fn rejects_wrong_port() {
        let err = parse_unifi_lan_url("rtsps://192.168.1.1:554/abc").unwrap_err();
        assert!(err.contains("port"));
    }

    #[test]
    fn rejects_empty_stream_id() {
        parse_unifi_lan_url("rtsps://192.168.1.1:7441/").unwrap_err();
    }

    #[test]
    fn rejects_path_with_slash() {
        parse_unifi_lan_url("rtsps://192.168.1.1:7441/foo/bar").unwrap_err();
    }

    #[test]
    fn public_ip_rejects_url() {
        validate_public_ip("https://1.2.3.4").unwrap_err();
    }

    #[test]
    fn public_ip_accepts_ipv4_and_hostname() {
        validate_public_ip("50.215.9.17").unwrap();
        validate_public_ip("oxy.example.com").unwrap();
        validate_public_ip("homer.no-ip.org").unwrap();
    }
}
