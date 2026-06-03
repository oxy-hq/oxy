//! Resolvers for world-model "Apps" configured via `integrations:` in
//! `config.yml`. Each resolver scans the config for the matching variant
//! and resolves its `*_var` field through the [`SecretsManager`].
//!
//! Returns `Ok(None)` when no matching integration is configured —
//! callers (Toast webhook, OpenWeatherMap proxy, BestTime proxy) handle
//! the missing-config case with their existing 503 / accept-and-skip
//! behavior. The hard `OxyError` cases are reserved for: (a) an
//! integration is declared but its `*_var` secret cannot be resolved,
//! and (b) the secrets storage itself errors.

use oxy_shared::errors::OxyError;

use crate::adapters::secrets::SecretsManager;
use crate::config::model::{Config, IntegrationType};

/// Toast webhook secret + the list of restaurant GUIDs this workspace
/// is authorized to accept. Returns `None` when no `toast` integration
/// is declared.
pub async fn resolve_toast(
    config: &Config,
    secrets: &SecretsManager,
) -> Result<Option<(String, Vec<String>)>, OxyError> {
    let Some(toast) = config
        .integrations
        .iter()
        .find_map(|i| match &i.integration_type {
            IntegrationType::Toast(t) => Some(t),
            _ => None,
        })
    else {
        return Ok(None);
    };
    let secret = secrets
        .resolve_config_value(
            None,
            Some(&toast.webhook_secret_var),
            "webhook_secret",
            None,
        )
        .await?;
    Ok(Some((secret, toast.restaurant_guids.clone())))
}

/// OpenWeatherMap API key. Returns `None` when no `openweathermap`
/// integration is declared.
pub async fn resolve_openweather(
    config: &Config,
    secrets: &SecretsManager,
) -> Result<Option<String>, OxyError> {
    let Some(owm) = config
        .integrations
        .iter()
        .find_map(|i| match &i.integration_type {
            IntegrationType::OpenWeatherMap(o) => Some(o),
            _ => None,
        })
    else {
        return Ok(None);
    };
    let key = secrets
        .resolve_config_value(None, Some(&owm.api_key_var), "api_key", None)
        .await?;
    Ok(Some(key))
}

/// BestTime API key. Returns `None` when no `besttime` integration is
/// declared.
pub async fn resolve_besttime(
    config: &Config,
    secrets: &SecretsManager,
) -> Result<Option<String>, OxyError> {
    let Some(bt) = config
        .integrations
        .iter()
        .find_map(|i| match &i.integration_type {
            IntegrationType::BestTime(b) => Some(b),
            _ => None,
        })
    else {
        return Ok(None);
    };
    let key = secrets
        .resolve_config_value(None, Some(&bt.api_key_var), "api_key", None)
        .await?;
    Ok(Some(key))
}

/// UniFi camera API key. Returns `None` when no `unifi` integration is
/// declared.
pub async fn resolve_unifi(
    config: &Config,
    secrets: &SecretsManager,
) -> Result<Option<String>, OxyError> {
    let Some(unifi) = config
        .integrations
        .iter()
        .find_map(|i| match &i.integration_type {
            IntegrationType::Unifi(u) => Some(u),
            _ => None,
        })
    else {
        return Ok(None);
    };
    let key = secrets
        .resolve_config_value(None, Some(&unifi.api_key_var), "api_key", None)
        .await?;
    Ok(Some(key))
}
