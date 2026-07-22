//! Auto-compile after an operation changes what the workspace *contains*.
//!
//! Reads on the serve fleet come from the promoted revision in Postgres, not
//! the working copy — so changing files on disk changes nothing anyone sees
//! until a compile promotes a new revision. Compile was manual, and neither
//! pull nor restore triggered one, so a workspace could sit indefinitely
//! serving configuration that no longer existed on disk. That is how a
//! `reconcile.yml` with 24 checks kept being evaluated as 19
//! (oxygen-workspace-sync-bugs.md bug 4).
//!
//! The existing lazy self-heal does not cover this: it only fires when the
//! boundary produced *no usable config at all*. A stale-but-valid revision
//! deserialises fine, so nothing ever noticed.
//!
//! Deliberately triggered at the **mutation points** (pull, restore) rather
//! than by staleness-checking on the read path: detecting staleness per request
//! means resolving git HEAD on every request, which is exactly the workspace-FS
//! read the compile boundary exists to remove.

use oxy_git::GitClient;
use uuid::Uuid;

use crate::server::api::middlewares::workspace_context::enqueue_compile_deduped;

/// Enqueue a promoting compile after `branch`'s contents changed on disk.
///
/// Best-effort and non-blocking for the caller's result: a failure here leaves
/// the workspace exactly where it was before (serving the old revision), which
/// is the same state the manual-only flow had. Callers therefore ignore the
/// outcome rather than failing an otherwise-successful pull or restore.
///
/// Skipped only when `branch` is not the workspace's default: compile ships the
/// default branch, and `open_compiled_revision` routes non-default branches to
/// the working copy, so a draft branch needs no revision.
///
/// Note this deliberately does **not** skip the single-instance (`All`) role.
/// It is tempting to — the IDE hides the manual Compile button there — but that
/// is a UI decision about the *button*, not about the read path.
/// `open_compiled_revision` only falls through to FS on `All` for the legacy
/// nil-UUID workspace or a non-default branch; on the default branch it serves
/// `current_revision_id` like any other role. Skipping here would leave
/// precisely the stale-serving bug this exists to prevent.
pub async fn compile_after_content_change(
    workspace_id: Uuid,
    workspace_path: &std::path::Path,
    branch: &str,
    reason: &'static str,
) {
    // Pooled; the same connection helper every handler in this crate uses.
    let db = match oxy::database::client::establish_connection().await {
        Ok(db) => db,
        Err(e) => {
            tracing::warn!(?e, %workspace_id, "auto compile: DB connect failed");
            return;
        }
    };

    let Some(default_branch) =
        crate::server::default_branch::resolve_default_branch(&db, workspace_id).await
    else {
        // No resolvable default branch (blank / demo / no-remote). There is no
        // committed identity to ship, and minting a synthetic local revision
        // behind the user's back is not this function's job.
        return;
    };
    if branch != default_branch {
        tracing::debug!(
            %workspace_id,
            branch,
            default_branch,
            "auto compile: not the default branch; skipping"
        );
        return;
    }

    // Resolve the post-change HEAD so the revision is addressable by SHA. This
    // is what makes a redundant trigger cheap: `oxy-compile` short-circuits a
    // matching (workspace_id, git_sha) to a promote instead of a full compile.
    let (sha, _subject) = oxy::github::default_git_client()
        .get_branch_commit(workspace_path, &default_branch)
        .await;
    if sha.is_empty() {
        tracing::warn!(
            %workspace_id,
            default_branch,
            "auto compile: could not resolve HEAD; skipping"
        );
        return;
    }

    tracing::info!(%workspace_id, git_sha = %sha, reason, "auto compile: enqueueing after content change");
    enqueue_compile_deduped(&db, workspace_id, Some(sha), Some(default_branch), reason).await;
}
