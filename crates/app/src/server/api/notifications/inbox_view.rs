//! A page of someone's inbox, put back together.
//!
//! The signals (who, when, read or not) are in Oxy's database; what each one
//! says is in its org's own store. This reads a page of signals, then the
//! content for that page — one query per org on the page, never per row — and
//! the app each came from, so the reader sees one list.
//!
//! An org whose store cannot be read right now still lists its notifications,
//! with `title: null`: the person learns something is there, and the badge
//! count — which never touches an org's store — stays right.
//!
//! A platform notice (no app, an `oxy.*` kind) has no content row: its words
//! are rendered from Oxy's own records ([`super::platform::render`]).

use std::collections::{HashMap, HashSet};

use entity::{apps, notifications, organizations};
use oxy_oltp::inbox::{self, InboxMessage};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde::Serialize;
use uuid::Uuid;

use super::links::inbox_href;
use super::platform::render::{self, About};

/// The app a notification came from, as the inbox names it.
#[derive(Debug, Clone, Serialize)]
pub struct InboxApp {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InboxItem {
    pub id: Uuid,
    pub org_id: Uuid,
    pub kind: String,
    pub created_at: chrono::DateTime<chrono::FixedOffset>,
    pub read_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    /// `None` when the org's store could not be read for this page.
    pub title: Option<String>,
    pub body: Option<String>,
    /// Where tapping it lands, relative to whatever host the reader is on:
    /// a path in Oxygen's web app.
    pub href: Option<String>,
    /// The app-relative path exactly as the app gave it to `ctx.notify`
    /// (`/bookings/1`), for the app's own router — the bell inside the app.
    /// `None` for a platform notice and for a notification without a link.
    pub link: Option<String>,
    pub app: Option<InboxApp>,
}

/// Turn a page of signals into inbox items, in the page's order.
pub async fn items(
    db: &DatabaseConnection,
    page: Vec<notifications::Model>,
) -> Result<Vec<InboxItem>, DbErr> {
    let content = content_for(db, &page).await;
    let org_slugs = org_slugs(db, &page).await?;
    let apps = apps_for(db, &page).await?;
    let notices = platform_words(db, &page).await?;
    Ok(page
        .into_iter()
        .map(|n| {
            if render::is_platform(&n) {
                let words = About::of(&n)
                    .and_then(|a| notices.get(&a).cloned())
                    .unwrap_or_else(|| render::fallback(&n.kind));
                return InboxItem {
                    id: n.id,
                    org_id: n.org_id,
                    kind: n.kind,
                    created_at: n.created_at,
                    read_at: n.read_at,
                    title: Some(words.title),
                    body: words.body,
                    href: words.href,
                    link: None,
                    app: None,
                };
            }
            let msg = n.message_id.and_then(|id| content.get(&id));
            let app = n.app_id.and_then(|id| apps.get(&id)).cloned();
            let href = msg.and_then(|m| {
                let org = org_slugs.get(&n.org_id)?;
                inbox_href(
                    org,
                    app.as_ref().map(|a| a.slug.as_str()),
                    m.link.as_deref(),
                )
            });
            InboxItem {
                id: n.id,
                org_id: n.org_id,
                kind: n.kind,
                created_at: n.created_at,
                read_at: n.read_at,
                title: msg.map(|m| m.title.clone()),
                body: msg.and_then(|m| m.body.clone()),
                href,
                link: msg.and_then(|m| m.link.clone()),
                app,
            }
        })
        .collect())
}

/// What to put before every `href` for a reader on `host`: the admin host's
/// origin on a custom-app subdomain, nothing elsewhere.
///
/// A custom-app host rewrites every path it does not reserve into the app it
/// serves (`custom_apps_host_dispatch`), so a bell inside one app that opened
/// another app's `/customer-apps/…` path, or a platform notice's web-app path,
/// would land inside the current app instead. There the links go to the admin
/// host absolutely. With no admin host configured (`OXY_API_URL` unset, a
/// laptop) they stay relative: a link that may not resolve beats a made-up
/// host. Same rule as the shell context's `product_url_base`.
pub fn href_base(host: Option<&str>, admin_base: Option<String>) -> String {
    let on_app_host = host
        .and_then(oxy_app_core::custom_apps_host_dispatch::parse_subdomain)
        .is_some();
    match on_app_host {
        true => admin_base.unwrap_or_default(),
        false => String::new(),
    }
}

/// Prefix every relative `href` with `base` (see [`href_base`]). `link` is
/// left alone: it is relative to the app, for the app's own router.
pub fn absolutize(items: &mut [InboxItem], base: &str) {
    if base.is_empty() {
        return;
    }
    for item in items {
        if let Some(href) = item.href.as_mut().filter(|h| h.starts_with('/')) {
            *href = format!("{base}{href}");
        }
    }
}

/// The words for every platform notice on the page.
async fn platform_words(
    db: &DatabaseConnection,
    page: &[notifications::Model],
) -> Result<HashMap<About, render::Rendered>, DbErr> {
    let abouts: HashSet<About> = page.iter().filter_map(About::of).collect();
    if abouts.is_empty() {
        return Ok(HashMap::new());
    }
    render::render_all(db, &abouts).await
}

/// Content for the page, keyed by message id — one read per org. An org whose
/// store fails is logged and left out; its rows read as `title: null`.
async fn content_for(
    db: &DatabaseConnection,
    page: &[notifications::Model],
) -> HashMap<Uuid, InboxMessage> {
    let mut by_org: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
    // A platform notice's `message_id` is its send, with no content row.
    for n in page.iter().filter(|n| !render::is_platform(n)) {
        if let Some(m) = n.message_id {
            by_org.entry(n.org_id).or_default().insert(m);
        }
    }
    let mut out = HashMap::new();
    for (org_id, ids) in by_org {
        let ids: Vec<Uuid> = ids.into_iter().collect();
        match inbox::read(db, org_id, &ids).await {
            Ok(msgs) => out.extend(msgs.into_iter().map(|m| (m.id, m))),
            Err(e) => tracing::warn!(%org_id, error = %e, "inbox content unavailable for this org"),
        }
    }
    out
}

async fn org_slugs(
    db: &DatabaseConnection,
    page: &[notifications::Model],
) -> Result<HashMap<Uuid, String>, DbErr> {
    let ids: HashSet<Uuid> = page.iter().map(|n| n.org_id).collect();
    Ok(organizations::Entity::find()
        .filter(organizations::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|o| (o.id, o.slug))
        .collect())
}

async fn apps_for(
    db: &DatabaseConnection,
    page: &[notifications::Model],
) -> Result<HashMap<Uuid, InboxApp>, DbErr> {
    let ids: HashSet<Uuid> = page.iter().filter_map(|n| n.app_id).collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(apps::Entity::find()
        .filter(apps::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|a| {
            (
                a.id,
                InboxApp {
                    id: a.id,
                    slug: a.slug,
                    name: a.name,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(href: Option<&str>, link: Option<&str>) -> InboxItem {
        InboxItem {
            id: Uuid::nil(),
            org_id: Uuid::nil(),
            kind: "k".into(),
            created_at: chrono::Utc::now().fixed_offset(),
            read_at: None,
            title: Some("t".into()),
            body: None,
            href: href.map(str::to_string),
            link: link.map(str::to_string),
            app: None,
        }
    }

    #[test]
    fn hrefs_go_absolute_on_a_custom_app_host_and_stay_relative_elsewhere() {
        let admin = || Some("https://app.oxygen-hq.com".to_string());
        for host in [
            "acme--store.customer-apps.oxygen-hq.com",
            "staging--acme--store.customer-apps-dev.oxygen-hq.com:443",
        ] {
            assert_eq!(
                href_base(Some(host), admin()),
                "https://app.oxygen-hq.com",
                "{host}"
            );
        }
        assert_eq!(href_base(Some("app.oxygen-hq.com"), admin()), "");
        assert_eq!(
            href_base(Some("acme.oxygen-hq.com"), admin()),
            "",
            "an org subdomain serves the SPA"
        );
        assert_eq!(href_base(None, admin()), "");
        assert_eq!(
            href_base(Some("acme--store.customer-apps.oxygen-hq.com"), None),
            "",
            "no admin host configured: relative, not invented"
        );

        let mut items = vec![
            item(Some("/customer-apps/acme/store/lists/1"), Some("/lists/1")),
            item(Some("/acme/workspaces/w"), None),
            item(None, None),
        ];
        absolutize(&mut items, "https://app.oxygen-hq.com");
        assert_eq!(
            items[0].href.as_deref(),
            Some("https://app.oxygen-hq.com/customer-apps/acme/store/lists/1")
        );
        assert_eq!(
            items[0].link.as_deref(),
            Some("/lists/1"),
            "the app's own link is untouched"
        );
        assert_eq!(
            items[1].href.as_deref(),
            Some("https://app.oxygen-hq.com/acme/workspaces/w")
        );
        assert_eq!(items[2].href, None);
    }

    #[test]
    fn link_is_always_in_the_json_null_when_absent() {
        // The SDK reads a missing key as a server that predates `app_id`.
        let v = serde_json::to_value(item(None, None)).unwrap();
        assert_eq!(v.get("link"), Some(&serde_json::Value::Null));
    }
}
