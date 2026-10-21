//! What an app's pointer history (`app_environment_events`) and its builds'
//! `published_to` say about one build — the evidence a promote, a rollback
//! and the read-back routes decide on
//! (`internal-docs/2026-09-10-custom-app-environments-design.md` §3.1, §5.2).
//!
//! Every pointer move appends an event naming the environment and the build
//! (`custom_apps_environments::record_move`), and the migration that created
//! the table backfilled one per channel pointer. So "has staging ever served
//! this build" is a question about events, never about the pointer staging
//! holds now — and it stays answerable after staging moves on.

use chrono::{DateTime, FixedOffset};
use entity::{app_builds, app_environment_events};
use oxy_app_core::custom_app_environment::AppEnvironment;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
};
use uuid::Uuid;

/// Whether `environment` of `app_id` has ever pointed at `build`.
pub async fn has_served<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    environment: &AppEnvironment,
    build: Uuid,
) -> Result<bool, DbErr> {
    Ok(served_since(db, app_id, environment, build)
        .await?
        .is_some())
}

/// When `environment` last moved to `build` — the start of the stretch it
/// serves now, if it still does. `None` when it never did.
pub async fn served_since<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    environment: &AppEnvironment,
    build: Uuid,
) -> Result<Option<DateTime<FixedOffset>>, DbErr> {
    Ok(app_environment_events::Entity::find()
        .filter(app_environment_events::Column::AppId.eq(app_id))
        .filter(app_environment_events::Column::Environment.eq(environment.name()))
        .filter(app_environment_events::Column::BuildId.eq(build))
        .order_by_desc(app_environment_events::Column::At)
        .one(db)
        .await?
        .map(|event| event.at))
}

/// The build `environment` served most recently that is not `current` — what
/// a rollback without `--to` moves back to. Rolling back twice therefore
/// swaps the two newest builds back and forth, as an undo does. A pruned
/// build is never named: pruning sets its events' `build_id` to NULL
/// (`fk_app_environment_events_build_id`, `ON DELETE SET NULL`).
pub async fn previous_build<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    environment: &AppEnvironment,
    current: Option<Uuid>,
) -> Result<Option<Uuid>, DbErr> {
    let mut find = app_environment_events::Entity::find()
        .filter(app_environment_events::Column::AppId.eq(app_id))
        .filter(app_environment_events::Column::Environment.eq(environment.name()))
        .filter(app_environment_events::Column::BuildId.is_not_null());
    if let Some(current) = current {
        find = find.filter(app_environment_events::Column::BuildId.ne(current));
    }
    let latest: Option<Option<Uuid>> = find
        .select_only()
        .column(app_environment_events::Column::BuildId)
        .order_by_desc(app_environment_events::Column::At)
        .into_tuple()
        .one(db)
        .await?;
    Ok(latest.flatten())
}

/// The non-production environment one of `builds` came from, among those
/// production never served — a build staging moved off, or a sandbox's after
/// the sandbox was deleted. Read from `published_to`, or for a build older
/// than that column, from its first event. `None` when production served
/// every named build, or nothing records where one came from.
///
/// [`custom_apps_env_resolve::non_production_environment_serving`] answers
/// the same question for a build an environment serves now; this one is for
/// the build nothing serves any more.
///
/// [`custom_apps_env_resolve::non_production_environment_serving`]:
///     super::custom_apps_env_resolve::non_production_environment_serving
pub async fn non_production_origin<C: ConnectionTrait>(
    db: &C,
    app_id: Uuid,
    builds: &[Uuid],
) -> Result<Option<AppEnvironment>, DbErr> {
    if builds.is_empty() {
        return Ok(None);
    }
    let rows = app_builds::Entity::find()
        .filter(app_builds::Column::AppId.eq(app_id))
        .filter(app_builds::Column::Id.is_in(builds.iter().copied()))
        .order_by_desc(app_builds::Column::CreatedAt)
        .all(db)
        .await?;
    let events = app_environment_events::Entity::find()
        .filter(app_environment_events::Column::AppId.eq(app_id))
        .filter(app_environment_events::Column::BuildId.is_in(builds.iter().copied()))
        .order_by_asc(app_environment_events::Column::At)
        .all(db)
        .await?;
    let events: Vec<(Uuid, &str)> = events
        .iter()
        .filter_map(|e| Some((e.build_id?, e.environment.as_str())))
        .collect();
    Ok(rows
        .iter()
        .find_map(|build| origin_of(build.id, build.published_to.as_deref(), &events)))
}

/// One build's non-production origin, given `(build, environment)` for the
/// app's events, oldest first.
fn origin_of(
    build: Uuid,
    published_to: Option<&str>,
    events: &[(Uuid, &str)],
) -> Option<AppEnvironment> {
    let production = AppEnvironment::Production.name();
    let mut mine = events.iter().filter(|(b, _)| *b == build).map(|(_, e)| *e);
    if mine.clone().any(|e| e == production) {
        return None;
    }
    let named = published_to.or_else(|| mine.next())?;
    AppEnvironment::parse(named).filter(|env| *env != AppEnvironment::Production)
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Uuid = Uuid::from_u128(1);

    #[test]
    fn a_deleted_sandboxs_build_is_still_known_to_be_the_sandboxs() {
        assert_eq!(
            origin_of(B, Some("dev-a1"), &[]),
            AppEnvironment::parse("dev-a1")
        );
        assert_eq!(
            origin_of(B, Some("staging"), &[(B, "staging")]),
            Some(AppEnvironment::Staging)
        );
    }

    #[test]
    fn production_ever_serving_it_makes_it_no_ones_secret() {
        let served = [(B, "staging"), (B, "production")];
        assert_eq!(origin_of(B, Some("staging"), &served), None);
    }

    #[test]
    fn a_build_older_than_the_column_falls_back_to_its_first_event() {
        let other = Uuid::from_u128(2);
        let events = [(other, "staging"), (B, "dev-b2"), (B, "staging")];
        assert_eq!(origin_of(B, None, &events), AppEnvironment::parse("dev-b2"));
        assert_eq!(origin_of(B, None, &[]), None, "nothing records it");
    }
}
