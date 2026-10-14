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

use std::collections::{HashMap, HashSet};

use entity::{apps, notifications, organizations};
use oxy_oltp::inbox::{self, InboxMessage};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};
use serde::Serialize;
use uuid::Uuid;

use super::links::inbox_href;

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
    /// Where tapping it lands, relative to whatever host the reader is on.
    pub href: Option<String>,
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
    Ok(page
        .into_iter()
        .map(|n| {
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
                app,
            }
        })
        .collect())
}

/// Content for the page, keyed by message id — one read per org. An org whose
/// store fails is logged and left out; its rows read as `title: null`.
async fn content_for(
    db: &DatabaseConnection,
    page: &[notifications::Model],
) -> HashMap<Uuid, InboxMessage> {
    let mut by_org: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
    for n in page {
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
