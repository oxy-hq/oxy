//! Keeps each workspace's `origin/*` remote-tracking refs warm.
//!
//! Everything that reports remote state — the Compile button's "Up to date"
//! badge, `revision-info`'s ahead/behind counts, the branch list — reads the
//! *locally cached* tracking ref (`git rev-parse origin/<branch>`), which is
//! only refreshed by an explicit fetch or pull. Nothing refreshed it on a
//! schedule, so those surfaces silently answered from whenever the user last
//! happened to fetch. That is how a workspace reported `behind: 0` while
//! demonstrably missing a commit that had been on origin for half an hour, and
//! why a freshness badge could not be trusted at all
//! (oxygen-workspace-sync-bugs.md bugs 1 and 3).
//!
//! This loop is deliberately **read-only with respect to the working copy**: it
//! runs `git fetch`, which updates `refs/remotes/*` and `FETCH_HEAD` and never
//! touches `HEAD`, the index, or any tracked file. It is therefore safe to run
//! underneath an editing user, mid-rebase, or on a dirty tree — unlike a pull,
//! which is why this is not one.
//!
//! Runs only where a working copy exists (`Ide` / `All`). A `Serve` replica has
//! no clone to fetch into, and a `Worker` has no reason to.

use std::time::Duration;

use oxy_git::GitClient;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use tracing::{debug, info, warn};

use crate::server::role_manifest::{Role, current_process_role};

const INTERVAL_ENV: &str = "OXY_GIT_FETCH_INTERVAL_SECS";
const DEFAULT_INTERVAL_SECS: u64 = 300;

/// Floor on the interval. Each tick is one `git fetch` per git-backed
/// workspace against the forge, so a small value turns into sustained
/// outbound traffic and, on GitHub App installs, rate-limit pressure.
const MIN_INTERVAL_SECS: u64 = 60;

/// Cap on how long a single workspace's fetch may take before it is abandoned
/// for this tick. An unreachable host would otherwise stall every workspace
/// behind it, so one bad remote cannot starve the rest.
const PER_WORKSPACE_TIMEOUT: Duration = Duration::from_secs(30);

pub struct GitFetchMaintenanceConfig {
    pub interval: Duration,
}

impl GitFetchMaintenanceConfig {
    pub fn from_env() -> Self {
        let secs = std::env::var(INTERVAL_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_INTERVAL_SECS)
            .max(MIN_INTERVAL_SECS);
        Self {
            interval: Duration::from_secs(secs),
        }
    }
}

/// Spawn the detached fetch loop. No-op on roles without a working copy.
pub fn spawn_git_fetch_maintenance(config: GitFetchMaintenanceConfig) {
    let role = current_process_role();
    if !matches!(role, Role::Ide | Role::All) {
        debug!(role = role.as_str(), "git fetch maintenance: not this role");
        return;
    }

    tokio::spawn(async move {
        let db = match oxy::database::client::establish_connection().await {
            Ok(db) => db,
            Err(e) => {
                warn!(
                    ?e,
                    "git fetch maintenance: DB connect failed; loop not started"
                );
                return;
            }
        };
        info!(
            interval_secs = config.interval.as_secs(),
            "git fetch maintenance: started"
        );

        let mut tick = tokio::time::interval(config.interval);
        // A sweep is sequential with a per-workspace timeout, so on a node
        // holding many workspaces with slow or unreachable remotes it can
        // outrun the interval. Under the default `Burst` behaviour the missed
        // ticks then fire back-to-back and the freshness sweep degenerates into
        // continuous fetching — exactly the load the timeout exists to bound.
        // `Delay` keeps the intended spacing from the end of each sweep.
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // The first tick fires immediately; skip it so startup isn't competing
        // with clone/migration work for the same remotes.
        tick.tick().await;
        loop {
            tick.tick().await;
            fetch_all_workspaces(&db).await;
        }
    });
}

async fn fetch_all_workspaces(db: &DatabaseConnection) {
    let workspaces = match entity::workspaces::Entity::find()
        .filter(entity::workspaces::Column::GitRemoteUrl.is_not_null())
        .all(db)
        .await
    {
        Ok(rows) => rows,
        Err(e) => {
            warn!(?e, "git fetch maintenance: workspace query failed");
            return;
        }
    };

    let (mut ok, mut failed) = (0u32, 0u32);
    for ws in workspaces {
        // Sequential on purpose: these are network calls against (usually) the
        // same forge, and this is a background freshness sweep with no deadline.
        // Fanning out would add rate-limit pressure to buy latency nobody waits on.
        match fetch_one(&ws).await {
            Ok(true) => ok += 1,
            Ok(false) => {}
            Err(e) => {
                failed += 1;
                // Debug, not warn: a workspace whose remote is unreachable or
                // whose token has lapsed would otherwise log on every tick
                // forever. The staleness is already visible in the UI, which is
                // the signal that matters.
                debug!(workspace_id = %ws.id, error = %e, "git fetch maintenance: fetch failed");
            }
        }
    }
    if ok > 0 || failed > 0 {
        debug!(ok, failed, "git fetch maintenance: sweep complete");
    }
}

/// Fetch one workspace's default branch. `Ok(false)` = nothing to do.
async fn fetch_one(ws: &entity::workspaces::Model) -> Result<bool, oxy_shared::errors::OxyError> {
    let Some(path) = ws.path.as_deref() else {
        return Ok(false);
    };
    let path = std::path::Path::new(path);
    // A replica may hold a row for a workspace it has never cloned.
    if !path.exists() {
        return Ok(false);
    }

    let git = oxy::github::default_git_client();
    if !git.is_git_repo(path) || !git.has_remote(path).await {
        return Ok(false);
    }

    let branch = git.get_default_branch(path).await;
    if branch.is_empty() {
        return Ok(false);
    }

    let token = oxy::github::github_token_for_workspace(ws).await?;
    let fetch = git.fetch_remote_ref(path, &branch, token.as_deref());
    match tokio::time::timeout(PER_WORKSPACE_TIMEOUT, fetch).await {
        Ok(result) => result.map(|()| true),
        Err(_) => Err(oxy_shared::errors::OxyError::RuntimeError(format!(
            "fetch timed out after {}s",
            PER_WORKSPACE_TIMEOUT.as_secs()
        ))),
    }
}
