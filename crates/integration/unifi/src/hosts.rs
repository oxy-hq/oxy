//! `/v1/hosts` — list controllers accessible to this API key.
//!
//! Empirical verification (admin-only key against a 17-controller account):
//! 200 across all hosts. Each host has `id` (the `console_id` used in
//! the Protect proxy), `hardwareId`, `ipAddress` (public IP), `owner`
//! (whether the API key is the host owner — required for the connector
//! proxy), and a `reportedState` with hostname / hardware / etc.

use crate::{UnifiClient, UnifiError, UnifiResult};
use serde::{Deserialize, Serialize};

/// Minimal `Host` shape — we project just the fields the cameras crate
/// needs. The real response has dozens more.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: String,
    #[serde(rename = "hardwareId")]
    pub hardware_id: Option<String>,
    #[serde(default, rename = "ipAddress")]
    pub ip_address: Option<String>,
    #[serde(default)]
    pub owner: bool,
    #[serde(rename = "userData", default)]
    pub user_data: Option<HostUserData>,
    #[serde(rename = "reportedState", default)]
    pub reported_state: Option<HostReportedState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostUserData {
    #[serde(default)]
    pub controllers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostReportedState {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub hardware: Option<HostHardware>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostHardware {
    #[serde(default)]
    pub shortname: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HostsResp {
    data: Vec<Host>,
}

#[derive(Debug, Deserialize)]
struct HostResp {
    data: Host,
}

impl UnifiClient {
    /// `GET /v1/hosts` — list every controller this key can see.
    pub async fn list_hosts(&self) -> UnifiResult<Vec<Host>> {
        let url = self.base_url().join("/v1/hosts").expect("static path");
        let resp = self
            .http()
            .get(url)
            .header("X-API-KEY", self.api_key())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| UnifiError::Transport(e.to_string()))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| UnifiError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        let parsed: HostsResp =
            serde_json::from_str(&body).map_err(|e| UnifiError::Decode(e.to_string()))?;
        Ok(parsed.data)
    }

    /// `GET /v1/hosts/{id}` — per-host detail. Most importantly the
    /// authoritative `ipAddress` (public) which `list_hosts` may omit
    /// for inactive hosts.
    pub async fn get_host(&self, host_id: &str) -> UnifiResult<Host> {
        let path = format!("/v1/hosts/{host_id}");
        let url = self.base_url().join(&path).expect("static path");
        let resp = self
            .http()
            .get(url)
            .header("X-API-KEY", self.api_key())
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| UnifiError::Transport(e.to_string()))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| UnifiError::Transport(e.to_string()))?;
        if !status.is_success() {
            return Err(map_status(status.as_u16(), &body));
        }
        let parsed: HostResp =
            serde_json::from_str(&body).map_err(|e| UnifiError::Decode(e.to_string()))?;
        Ok(parsed.data)
    }
}

pub(crate) fn map_status(status: u16, body: &str) -> UnifiError {
    match status {
        401 | 403 => UnifiError::Forbidden(body.to_string()),
        404 => UnifiError::NotFound(body.to_string()),
        429 => UnifiError::RateLimited {
            retry_after_secs: None,
        },
        _ => UnifiError::Unexpected {
            status,
            body: body.chars().take(500).collect(),
        },
    }
}
