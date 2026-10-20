//! Builds a **sandbox agent token** published
//! (`app_builds.published_token_id`) — to a sandbox of its own, or as a draft
//! to staging — and the two rules every such build is held to (sandbox agent
//! credential design, "Staging option", §10.5).
//!
//! An agent publishes builds nobody approved. So each one remembers who
//! published it, and:
//!
//! 1. **Production never falls back to it** ([`production_fallback`]). An app
//!    with no production build of its own — never promoted, or unpublished —
//!    serves staging's build on the production path, HTML and functions
//!    alike. Not when a token published that build: production then serves
//!    nothing, exactly as an app with neither build. The draft's guard that
//!    the app is live is asked when the draft is published; this is what
//!    holds once the app is unpublished afterwards. (Only a draft is ever
//!    staging's build: nothing moves staging's pointer to a sandbox build.)
//! 2. **No blind promote ships it** ([`AgentBuilt::describe`], asked of
//!    `custom_apps_build_state::BuildState::promotable`). Promote, batch
//!    promote, promote latest and a rollback to it move production's pointer
//!    to whatever build they find, with no look at whose it is — promote
//!    latest takes an app's newest build whatever served it, and a rollback
//!    names any retained build, so a token's **sandbox** build was as
//!    shippable as its draft. Each refuses a marked build with
//!    `409 draft_published_by_agent`, naming the token, its minter and the
//!    environment the build was published to. (The code says "draft" for a
//!    sandbox build too: clients key on it.) A person who publishes the build
//!    under their own name has a build that is theirs, and that one promotes
//!    as any build does.
//!
//! The mark is set on every build a sandbox agent token publishes
//! (`custom_apps_sandboxes::agent_publish::author`) and is never updated, so
//! both rules read `NULL` — and change nothing — for every build a person or
//! a CI job published. Nothing a sandbox does with its own build reads it: a
//! sandbox serves what its row names.

use std::collections::HashMap;
use std::sync::OnceLock;

use entity::app_builds;
use sea_orm::QuerySelect;
use sea_orm::{ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::build_state::Provenance;

/// The code a promote or a rollback refuses a token-published build with.
pub const PROMOTE_REFUSED: &str = "draft_published_by_agent";

/// Whether a build is marked, per build id, for the 60 s every other step of
/// the serve chain is cached. The mark is written with the row and never
/// changed, so an entry can only ever be refreshed with the same answer.
type MarkCache = crate::cache::TtlLru<Uuid, bool>;

fn mark_cache() -> &'static MarkCache {
    static CACHE: OnceLock<MarkCache> = OnceLock::new();
    CACHE.get_or_init(crate::cache::standard_cache)
}

/// Whether a sandbox agent token published build `build_pk`. One primary-key
/// read of one column, then cached: only a request that takes production's
/// fallback asks, never one of an app with a production build.
pub async fn is_agent_built<C: ConnectionTrait>(db: &C, build_pk: Uuid) -> Result<bool, DbErr> {
    if let Some(known) = mark_cache().get(&build_pk) {
        return Ok(known);
    }
    let token: Option<Option<Uuid>> = app_builds::Entity::find_by_id(build_pk)
        .select_only()
        .column(app_builds::Column::PublishedTokenId)
        .into_tuple()
        .one(db)
        .await?;
    let marked = Provenance::from_token(token.flatten()).is_agent_built();
    mark_cache().insert(build_pk, marked);
    Ok(marked)
}

/// Read the mark of every build in `builds` not already cached, in one query,
/// into the cache [`is_agent_built`] answers from. For a reader resolving many
/// apps' production build at once (the notification catalogue), so it pays one
/// read rather than one per never-promoted app. The answer is the same: a
/// build with no row is unmarked, as there.
pub async fn prime_marks<C: ConnectionTrait>(
    db: &C,
    builds: impl IntoIterator<Item = Uuid>,
) -> Result<(), DbErr> {
    let unknown: Vec<Uuid> = builds
        .into_iter()
        .filter(|b| mark_cache().get(b).is_none())
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    let marked: HashMap<Uuid, bool> = app_builds::Entity::find()
        .select_only()
        .column(app_builds::Column::Id)
        .column(app_builds::Column::PublishedTokenId)
        .filter(app_builds::Column::Id.is_in(unknown.iter().copied()))
        .into_tuple::<(Uuid, Option<Uuid>)>()
        .all(db)
        .await?
        .into_iter()
        .map(|(build, token)| (build, Provenance::from_token(token).is_agent_built()))
        .collect();
    for build in unknown {
        let mark = marked.get(&build).copied().unwrap_or(false);
        mark_cache().insert(build, mark);
    }
    Ok(())
}

/// Make a build's mark known, as a first request would have left it: for a
/// test that resolves with no database behind it.
#[cfg(test)]
pub(crate) fn remember(build_pk: Uuid, marked: bool) {
    mark_cache().insert(build_pk, marked);
}

/// What production serves when it has no build of its own: `staging`'s build,
/// as it always has — unless a sandbox agent token published that build, and
/// then nothing. `Err` when the mark cannot be read: the caller serves nothing
/// rather than a build nobody approved.
pub async fn production_fallback<C: ConnectionTrait>(
    db: &C,
    staging: Option<Uuid>,
) -> Result<Option<Uuid>, DbErr> {
    match staging {
        Some(build) if is_agent_built(db, build).await? => Ok(None),
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mark is read from the cache once it is known: a build is asked of
    /// the database at most once a minute, whatever serves it.
    #[test]
    fn a_known_mark_is_answered_from_the_cache() {
        let (marked, plain) = (Uuid::new_v4(), Uuid::new_v4());
        assert_eq!(mark_cache().get(&marked), None);
        mark_cache().insert(marked, true);
        mark_cache().insert(plain, false);
        assert_eq!(mark_cache().get(&marked), Some(true));
        assert_eq!(mark_cache().get(&plain), Some(false));
    }
}
