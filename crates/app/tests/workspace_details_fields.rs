//! Integration-style coverage for the per-workspace git state fields on
//! `WorkspaceDetailsResponse`. The existing app-level test infrastructure
//! only exercises the CLI binary (see `build.rs` / `run.rs` / `test.rs`) —
//! spinning up a full HTTP server with auth + DB would require standing up
//! PostgreSQL fixtures, so these tests drive the response builder directly.
//! That gives us the same field-level coverage the plan calls for without
//! inventing a new test harness.

use oxy_app::api::workspaces::{
    GitCapabilities, GitMode, build_workspace_details_response,
    build_workspace_details_response_for_uninitialized_local,
};
use std::process::Command;
use tempfile::TempDir;
use uuid::Uuid;

/// Initialise a real git repository in `dir` via the system `git` binary.
/// `LocalGitService::is_git_repo` only checks for the presence of a `.git`
/// directory, but `get_default_branch` / `has_remote` shell out to real
/// `git` commands, so we need an actual repo on disk.
fn init_git_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("failed to invoke git");
        assert!(
            status.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&status.stderr)
        );
    };
    run(&["init", "-b", "main"]);
    run(&["config", "user.email", "oxy-test@example.com"]);
    run(&["config", "user.name", "Oxy Test"]);
    run(&["commit", "--allow-empty", "-m", "initial"]);
}

#[tokio::test]
async fn git_enabled_workspace_reports_local_mode() {
    // Clear GIT_REPOSITORY_URL so git_mode is driven solely by the repo's
    // own remote configuration (which we haven't added).
    // SAFETY: tests run in the same process; this is best-effort. We do not
    // restore the value because no other test in this file depends on it.
    unsafe {
        std::env::remove_var("GIT_REPOSITORY_URL");
    }

    let tmp = TempDir::new().expect("tempdir");
    init_git_repo(tmp.path());

    let workspace_id = Uuid::new_v4();
    let resp = build_workspace_details_response(workspace_id, "test-workspace", tmp.path(), false)
        .await
        .expect("builder returned error");

    let body = resp.0;
    assert_eq!(body.id, workspace_id);
    assert_eq!(body.name, "test-workspace");
    assert!(
        body.workspace_error.is_none(),
        "no workspace_error expected"
    );
    // .git exists, no remote, GIT_REPOSITORY_URL cleared → Local mode.
    assert_eq!(
        body.git_mode,
        GitMode::Local,
        "git_mode should be Local for a git repo without a remote"
    );
    assert!(body.capabilities.can_commit, "Local mode allows commits");
    assert!(
        !body.capabilities.can_push,
        "Local mode (no remote) cannot push"
    );
    assert_eq!(body.default_branch, "main");
    assert!(
        !body.protected_branches.is_empty(),
        "protected_branches should default to [default_branch]"
    );
    assert!(
        body.protected_branches.contains(&"main".to_string()),
        "protected_branches should contain default branch, got {:?}",
        body.protected_branches
    );
    assert!(
        !body.requires_local_setup,
        "requires_local_setup must be false here"
    );
    let branch = body.active_branch.expect("active_branch expected");
    assert_eq!(branch.name, "main");
}

#[tokio::test]
async fn missing_workspace_directory_reports_workspace_error() {
    let tmp = TempDir::new().expect("tempdir");
    let missing = tmp.path().join("does-not-exist");
    assert!(!missing.exists());

    let workspace_id = Uuid::new_v4();
    let resp = build_workspace_details_response(workspace_id, "gone", &missing, false)
        .await
        .expect("builder returned error");

    let body = resp.0;
    assert_eq!(body.id, workspace_id);
    let err = body
        .workspace_error
        .as_ref()
        .expect("workspace_error should be set");
    assert!(
        !err.is_empty(),
        "workspace_error message should be non-empty"
    );
    assert_eq!(
        body.git_mode,
        GitMode::None,
        "git_mode must be None when dir missing"
    );
    assert!(
        !body.capabilities.can_commit,
        "no capabilities when dir missing"
    );
    assert!(
        !body.capabilities.can_push,
        "no capabilities when dir missing"
    );
    assert_eq!(body.default_branch, "main");
    assert_eq!(body.protected_branches, vec!["main".to_string()]);
    assert!(
        body.active_branch.is_none(),
        "active_branch should be None when dir missing"
    );
    assert!(
        !body.requires_local_setup,
        "requires_local_setup must be false here"
    );
}

#[tokio::test]
async fn local_mode_forces_git_mode_none_even_with_dot_git_present() {
    // User runs `oxy start` inside a directory that already has a .git
    // folder (e.g. the oxy source checkout). detect_git_mode would
    // otherwise report Local; we must force None so the frontend does
    // not light up UI for routes that are not mounted.
    let tmp = TempDir::new().expect("tempdir");
    init_git_repo(tmp.path());

    let workspace_id = Uuid::new_v4();
    let resp = build_workspace_details_response(
        workspace_id,
        "local-workspace",
        tmp.path(),
        true, // git_disabled
    )
    .await
    .expect("builder returned error");

    let body = resp.0;
    assert_eq!(
        body.git_mode,
        GitMode::None,
        "local mode must force git_mode=None regardless of on-disk .git"
    );
    let expected_caps: GitCapabilities = GitMode::None.into();
    assert_eq!(
        body.capabilities, expected_caps,
        "capabilities must exactly match GitMode::None"
    );
    assert!(
        body.active_branch.is_none(),
        "active_branch must be None when git is disabled"
    );
    assert!(
        !body.requires_local_setup,
        "requires_local_setup must be false here"
    );
}

#[tokio::test]
async fn uninitialized_local_workspace_returns_requires_local_setup_true() {
    let workspace_id = Uuid::new_v4();
    let resp = build_workspace_details_response_for_uninitialized_local(workspace_id, "local");

    let body = resp.0;
    assert!(body.requires_local_setup);
    assert_eq!(body.git_mode, GitMode::None);
    assert!(body.workspace_error.is_none());
    assert_eq!(body.name, "local");
}
