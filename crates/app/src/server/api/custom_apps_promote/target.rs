//! Which build a production move is aimed at, and when that is decided.
//!
//! A caller that already holds a build id names it ([`BuildRef::Id`]). The
//! console's *Make Live* (`Draft`) and *promote latest* (`Newest`) instead
//! name a build by what it is: staging's pointer, the app's newest row. Read
//! before the move, such a name goes stale — a draft publish landing between
//! the read and the app's row lock would promote the build it replaced. So
//! the move resolves it **under the lock** ([`super::production`]): the
//! build it admitted and migrated unlocked is checked against what the name
//! means now, and a different answer starts the move over for the newer build.

use entity::{app_builds, apps};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use super::error::PromoteError;
use super::production::load_build;

/// The build a production move goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildRef {
    /// `app_builds.id`, as given.
    Id(Uuid),
    /// The build staging serves (`apps.draft_build_id`) when the row is read.
    Draft,
    /// The app's newest build, whatever served it.
    Newest,
}

impl From<Uuid> for BuildRef {
    fn from(build: Uuid) -> Self {
        Self::Id(build)
    }
}

impl BuildRef {
    /// The build this names, with `row` the app as `conn` sees it now: for a
    /// move's final answer, `conn` is the transaction holding `row` locked.
    pub async fn resolve<C: ConnectionTrait>(
        &self,
        conn: &C,
        row: &apps::Model,
    ) -> Result<app_builds::Model, PromoteError> {
        match self {
            Self::Id(build) => load_build(conn, row.id, *build).await,
            Self::Draft => {
                let draft = row
                    .draft_build_id
                    .ok_or_else(|| PromoteError::NothingToPromote("staging".to_string()))?;
                load_build(conn, row.id, draft).await
            }
            Self::Newest => newest_build(conn, row.id).await,
        }
    }

    /// What the move says when the name kept changing under it.
    pub fn moved(&self, expected: Uuid, actual: Uuid) -> PromoteError {
        let what = match self {
            Self::Id(_) | Self::Draft => "staging's draft",
            Self::Newest => "the app's newest build",
        };
        PromoteError::TargetMoved {
            environment: what.to_string(),
            expected: expected.to_string(),
            actual: actual.to_string(),
        }
    }
}

async fn newest_build<C: ConnectionTrait>(
    conn: &C,
    app_id: Uuid,
) -> Result<app_builds::Model, PromoteError> {
    app_builds::Entity::find()
        .filter(app_builds::Column::AppId.eq(app_id))
        .order_by_desc(app_builds::Column::CreatedAt)
        .one(conn)
        .await
        .map_err(|e| PromoteError::internal("find the newest build", e))?
        .ok_or_else(|| PromoteError::NothingToPromote("the app".to_string()))
}
