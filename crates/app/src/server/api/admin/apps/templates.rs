//! Template gallery endpoint. The admin Create-new flow shows a
//! picker of available templates; this endpoint serves the list.
//! Admin-gated (the parent mount already wraps in
//! `oxy_app_admin_guard`).
//!
//! No screenshot endpoint yet — every `template.json` declares only
//! `{id, name, description}`. Re-introduce a `screenshot_url` field on
//! `TemplateListItem` + a `GET .../templates/{id}/screenshot` handler
//! when the first PNG ships; the UI's `<img>` branch can come back at
//! the same time. The bundle filter in `customer_app_template/mod.rs`
//! already drops `screenshot.png` from the rendered scaffold.

use axum::Json;
use serde::Serialize;

use crate::customer_app_template::registry;

#[derive(Debug, Serialize)]
pub struct TemplateListItem {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// `GET /api/customer-apps/templates` (mounted under the
/// `oxy_app_admin_guard` nest in `router/global.rs`, NOT the
/// `oxy_owner_guard` `/api/admin/...` surface).
pub async fn list_templates() -> Json<Vec<TemplateListItem>> {
    let items: Vec<TemplateListItem> = registry::templates()
        .values()
        .map(|t| TemplateListItem {
            id: t.meta.id.clone(),
            name: t.meta.name.clone(),
            description: t.meta.description.clone(),
        })
        .collect();
    Json(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn list_returns_all_registered_templates() {
        let response = list_templates().await;
        let items = response.0;
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert!(
            ids.contains(&"vite"),
            "vite must be in the list; got {:?}",
            ids
        );
        assert!(
            ids.contains(&"dashboard"),
            "dashboard must be in the list; got {:?}",
            ids
        );
        assert!(
            ids.contains(&"single-store"),
            "single-store must be in the list; got {:?}",
            ids
        );
    }
}
