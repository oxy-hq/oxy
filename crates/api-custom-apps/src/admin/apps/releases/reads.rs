//! The pointer-history reads: production's events in the window (newest
//! first, capped, then reversed — so a cap drops the oldest, never the release
//! serving now), the one event serving at the window's start, the other
//! environments' events, and the labels of whoever moved them.

use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, Utc};
use entity::users;
use sea_orm::{
    ColumnTrait, DatabaseConnection, DbErr, EntityTrait, FromQueryResult, QueryFilter, Statement,
};
use uuid::Uuid;

use super::timeline::Event;

/// Production events one read returns. Past it the oldest are dropped and the
/// answer says so.
pub const MAX_PRODUCTION_EVENTS: usize = 200;
/// Staging and sandbox events one read returns, newest kept.
pub const MAX_OTHER_EVENTS: usize = 300;

/// An event and the build it names, when that build still exists.
#[derive(Debug, Clone, FromQueryResult)]
pub struct EventRow {
    pub id: Uuid,
    pub environment: String,
    pub action: String,
    pub at: DateTime<FixedOffset>,
    pub build_id: Option<Uuid>,
    pub actor: Option<Uuid>,
    pub publish_id: Option<String>,
    pub commit_sha: Option<String>,
    pub source_branch: Option<String>,
    pub source_repo: Option<String>,
    pub published_via: Option<String>,
    pub agent_built: Option<bool>,
}

impl EventRow {
    pub fn event(&self) -> Event {
        Event {
            id: self.id,
            environment: self.environment.clone(),
            action: self.action.clone(),
            at: self.at.with_timezone(&Utc),
            build_id: self.build_id,
            actor: self.actor,
        }
    }
}

const SELECT: &str = "SELECT e.id, e.environment, e.action, e.at, e.build_id, e.actor, \
       b.build_id AS publish_id, b.commit_sha, b.source_branch, b.source_repo, \
       b.published_via, (b.published_token_id IS NOT NULL) AS agent_built \
     FROM app_environment_events e LEFT JOIN app_builds b ON b.id = e.build_id \
     WHERE e.app_id = $1 ";

/// Served by `(app_id, environment, at DESC)`.
fn production_sql() -> String {
    format!(
        "{SELECT}AND e.environment = 'production' AND e.at >= $2 \
         ORDER BY e.at DESC, e.id DESC LIMIT $3"
    )
}

fn anchor_sql() -> String {
    format!(
        "{SELECT}AND e.environment = 'production' AND e.at < $2 \
         ORDER BY e.at DESC, e.id DESC LIMIT 1"
    )
}

fn other_sql() -> String {
    format!(
        "{SELECT}AND e.environment <> 'production' AND e.at >= $2 \
         ORDER BY e.at DESC, e.id DESC LIMIT $3"
    )
}

async fn rows(
    db: &DatabaseConnection,
    sql: String,
    app_id: Uuid,
    since: DateTime<Utc>,
    limit: Option<usize>,
) -> Result<Vec<EventRow>, DbErr> {
    let mut values = vec![app_id.into(), since.into()];
    if let Some(limit) = limit {
        values.push((limit as i64).into());
    }
    let stmt = Statement::from_sql_and_values(db.get_database_backend(), sql, values);
    EventRow::find_by_statement(stmt).all(db).await
}

/// What the pointer-history reads returned.
pub struct History {
    /// Production's, oldest first.
    pub production: Vec<EventRow>,
    pub anchor: Option<EventRow>,
    /// Staging's and every sandbox's, oldest first. Empty without reach.
    pub other: Vec<EventRow>,
    pub production_truncated: bool,
    pub other_truncated: bool,
}

/// Read the history of `app_id` since `since`; the other environments' only
/// when `reach`.
pub async fn history(
    db: &DatabaseConnection,
    app_id: Uuid,
    since: DateTime<Utc>,
    reach: bool,
) -> Result<History, DbErr> {
    let fetch = MAX_PRODUCTION_EVENTS + 1;
    let mut production = rows(db, production_sql(), app_id, since, Some(fetch)).await?;
    let production_truncated = production.len() > MAX_PRODUCTION_EVENTS;
    production.truncate(MAX_PRODUCTION_EVENTS);
    production.reverse();
    let anchor = rows(db, anchor_sql(), app_id, since, None).await?.pop();
    let (mut other, other_truncated) = if reach {
        let fetch = MAX_OTHER_EVENTS + 1;
        let mut other = rows(db, other_sql(), app_id, since, Some(fetch)).await?;
        let cut = other.len() > MAX_OTHER_EVENTS;
        other.truncate(MAX_OTHER_EVENTS);
        (other, cut)
    } else {
        (Vec::new(), false)
    };
    other.reverse();
    Ok(History {
        production,
        anchor,
        other,
        production_truncated,
        other_truncated,
    })
}

/// The label (`users.label()`) of each actor, as the build history shows a
/// publisher. A publish token reading its own app sees them too, as it sees
/// `/builds`' `published_by_email`: this is not a people route.
pub async fn actor_labels(
    db: &DatabaseConnection,
    ids: Vec<Uuid>,
) -> Result<HashMap<Uuid, String>, DbErr> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    Ok(users::Entity::find()
        .filter(users::Column::Id.is_in(ids))
        .all(db)
        .await?
        .into_iter()
        .map(|u| (u.id, u.label().to_string()))
        .collect())
}
