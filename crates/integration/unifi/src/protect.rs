//! `/v1/connector/consoles/{console_id}/proxy/protect/integration/v1/*`
//!
//! Owner-only. Returns 403 ("user is not the owner of this host")
//! when the API key holder only has admin permission. The only way to
//! programmatically fetch RTSPS URLs is through this surface.

use crate::{UnifiClient, UnifiError, UnifiResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RtspsStream {
    /// The ready-to-use stream URL. Routes through Ubiquiti's relay so
    /// it works from anywhere — no port-forwarding required.
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct RtspsStreamResp {
    #[serde(default)]
    data: Option<RtspsStreamData>,
    // The endpoint's response shape isn't fully documented; some
    // tenants get `data: {...}`, others get the fields at top level.
    // We try both.
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "rtspsUrl")]
    rtsps_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RtspsStreamData {
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "rtspsUrl")]
    rtsps_url: Option<String>,
}

impl UnifiClient {
    /// `GET /v1/connector/consoles/{console_id}/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream`
    ///
    /// Returns the streamable RTSPS URL for one camera. Requires owner
    /// permission on the console.
    pub async fn get_camera_rtsps(
        &self,
        console_id: &str,
        camera_id: &str,
    ) -> UnifiResult<RtspsStream> {
        let path = format!(
            "/v1/connector/consoles/{console_id}/proxy/protect/integration/v1/cameras/{camera_id}/rtsps-stream"
        );
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
            return Err(crate::hosts::map_status(status.as_u16(), &body));
        }
        let parsed: RtspsStreamResp =
            serde_json::from_str(&body).map_err(|e| UnifiError::Decode(e.to_string()))?;

        let url = parsed
            .data
            .as_ref()
            .and_then(|d| d.url.clone().or_else(|| d.rtsps_url.clone()))
            .or(parsed.url)
            .or(parsed.rtsps_url)
            .ok_or_else(|| UnifiError::Decode("rtsps-stream response missing url field".into()))?;

        Ok(RtspsStream { url })
    }
}
