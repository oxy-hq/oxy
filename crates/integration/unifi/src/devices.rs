//! `/v1/devices` — full device inventory across the account.
//!
//! Groups devices by host. The Protect cameras have
//! `productLine = "protect"` and `shortname` like "UVC G4 Pro".
//! Their `id` field is the Protect ObjectId used by the connector
//! proxy. Empirically: admin-only keys can read this endpoint
//! (no owner permission required).

use crate::{UnifiClient, UnifiError, UnifiResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostDevices {
    #[serde(rename = "hostId")]
    pub host_id: String,
    #[serde(rename = "hostName", default)]
    pub host_name: Option<String>,
    #[serde(default)]
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub shortname: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(rename = "productLine", default)]
    pub product_line: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DevicesResp {
    data: Vec<HostDevices>,
}

impl UnifiClient {
    /// `GET /v1/devices` — every device, grouped by host.
    pub async fn list_devices(&self) -> UnifiResult<Vec<HostDevices>> {
        let url = self.base_url().join("/v1/devices").expect("static path");
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
            return Err(crate::hosts::map_status(status.as_u16(), &body));
        }
        let parsed: DevicesResp =
            serde_json::from_str(&body).map_err(|e| UnifiError::Decode(e.to_string()))?;
        Ok(parsed.data)
    }
}

impl Device {
    /// Heuristic — exclude UFP Viewports (display-only devices) from
    /// the camera list. Real cameras are `productLine == "protect"`
    /// AND `shortname` doesn't contain "Viewport".
    pub fn is_camera(&self) -> bool {
        self.product_line.as_deref() == Some("protect")
            && !self.shortname.as_deref().unwrap_or("").contains("Viewport")
    }
}
