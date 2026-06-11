//! `oxy compile` — Phase 1.6a entry point for the compile boundary.
//!
//! Walks the local workspace, parses every recognised YAML/SQL file,
//! and writes the result into the compile-boundary Postgres schema
//! (revisions + per-entity tables) as a single revision_id.
//! Phase 1.6a does NOT update `workspaces.current_revision_id`;
//! observation only.
//!
//! Requires `OXY_DATABASE_URL` set (same as `oxy worker` and the
//! cloud-mode `oxy serve`). Local-mode users running `oxy start`
//! already have an embedded Postgres available.

use std::path::PathBuf;

use clap::Parser;
use oxy::config::resolve_local_workspace_path;
use oxy::database::client::establish_connection;
use oxy::theme::StyledText;
use oxy_compile::{
    compile_workspace, compiler_version, CompileError, CompileOutcome, CompileRequest,
    FailureKind, RevisionKind, RevisionStatus,
};
use oxy_shared::errors::OxyError;
use uuid::Uuid;

use super::serve;

#[derive(Parser, Debug, Clone)]
pub struct CompileArgs {
    /// Path to the workspace directory. Defaults to walking up from
    /// the current directory until a `config.yml` is found (same
    /// resolution as `oxy build`).
    #[clap(long)]
    pub workspace_path: Option<PathBuf>,

    /// Workspace UUID to associate the compile with. Defaults to the
    /// nil UUID, which is the local-mode workspace ID (matches the
    /// row seeded by `oxy start` / `oxy serve --local`). In cloud
    /// mode this must be supplied.
    #[clap(long)]
    pub workspace_id: Option<Uuid>,

    /// Optional git SHA recorded on the revision. When omitted, the
    /// literal "local" is used — useful for working-copy compiles
    /// with uncommitted edits.
    #[clap(long)]
    pub git_sha: Option<String>,

    /// Optional branch name recorded on the revision.
    #[clap(long)]
    pub branch: Option<String>,

    /// Skip running database migrations. Use when another process
    /// (`oxy serve`, CI step) has already migrated the database.
    #[clap(long)]
    pub skip_migrations: bool,

    /// Print the structured outcome as JSON on stdout instead of the
    /// human summary. Useful for CI scripts.
    #[clap(long)]
    pub json: bool,

    /// Atomically promote the new revision into
    /// `workspaces.current_revision_id` when the compile succeeds.
    /// Off by default to preserve the observation-mode contract of
    /// the foundation PR; opt in when you want runtime reads to start
    /// using the new revision.
    #[clap(long)]
    pub promote: bool,

    /// Run the underlying enterprise migrations alongside the OSS set.
    /// Mirrors `oxy serve --enterprise`; defaults to false.
    #[clap(long)]
    pub enterprise: bool,
}

/// `oxy compile` handler.
#[tracing::instrument(skip_all)]
pub async fn run_compile(args: CompileArgs) -> Result<(), OxyError> {
    let workspace_path = match args.workspace_path.clone() {
        Some(p) => p,
        None => resolve_local_workspace_path()?,
    };

    if !workspace_path.exists() {
        return Err(OxyError::RuntimeError(format!(
            "workspace path not found: {}",
            workspace_path.display()
        )));
    }

    if !args.skip_migrations {
        tracing::info!("compile: running database migrations");
        serve::run_database_migrations(args.enterprise).await?;
    } else {
        tracing::info!("compile: --skip-migrations set");
    }

    let db = establish_connection()
        .await
        .map_err(|e| OxyError::RuntimeError(format!("database connection failed: {e}")))?;

    let workspace_id = args.workspace_id.unwrap_or(Uuid::nil());

    println!(
        "{} {}",
        "Compiling workspace".text(),
        format!(
            "id={} path={}",
            workspace_id,
            workspace_path.display()
        )
        .secondary()
    );

    let outcome = compile_workspace(CompileRequest {
        db: &db,
        workspace_id,
        workspace_path: &workspace_path,
        git_sha: args.git_sha.clone(),
        branch: args.branch.clone(),
        compiler_version: compiler_version(),
        promote: args.promote,
        kind: RevisionKind::Main,
        owner_user_id: None,
    })
    .await
    .map_err(compile_error_to_oxy)?;

    if args.json {
        let body = serde_json::to_string_pretty(&outcome)
            .map_err(|e| OxyError::RuntimeError(format!("serialize outcome failed: {e}")))?;
        println!("{}", body);
    } else {
        print_human_summary(&outcome);
    }

    // Non-zero exit when the compile recorded any failures.
    if matches!(outcome.status, RevisionStatus::Failed) {
        return Err(OxyError::RuntimeError(format!(
            "compile recorded {} failed file(s); revision_id={}",
            outcome.file_count_failed, outcome.revision_id
        )));
    }

    Ok(())
}

fn print_human_summary(outcome: &CompileOutcome) {
    let duration = outcome
        .finished_at
        .signed_duration_since(outcome.started_at);
    let duration_ms = duration.num_milliseconds().max(0);

    let status_line = match outcome.status {
        RevisionStatus::Ready => format!(
            "{} {} files compiled in {}ms",
            "✓ Compile succeeded".success(),
            outcome.file_count_compiled,
            duration_ms
        ),
        RevisionStatus::Failed => format!(
            "{} {} of {} files failed in {}ms",
            "✗ Compile recorded failures".error(),
            outcome.file_count_failed,
            outcome.file_count_seen,
            duration_ms
        ),
    };
    println!("{}", status_line);
    println!(
        "  revision_id  = {}",
        outcome.revision_id.to_string().tertiary()
    );
    println!(
        "  git_sha      = {}",
        outcome.git_sha.tertiary()
    );
    if let Some(branch) = &outcome.branch {
        println!("  branch       = {}", branch.tertiary());
    }
    println!("  files_seen   = {}", outcome.file_count_seen);
    println!("  files_done   = {}", outcome.file_count_compiled);
    println!("  files_failed = {}", outcome.file_count_failed);

    if !outcome.failures.is_empty() {
        println!("\n{}", "Failures:".error());
        for failure in &outcome.failures {
            println!(
                "  {} {} — {}",
                kind_label(&failure.kind).tertiary(),
                failure.path,
                failure.message
            );
        }
    }
}

fn kind_label(kind: &FailureKind) -> &'static str {
    match kind {
        FailureKind::Yaml => "[yaml]",
        FailureKind::Io => "[io]",
        FailureKind::Shape => "[shape]",
        FailureKind::Duplicate => "[dup]",
    }
}

fn compile_error_to_oxy(e: CompileError) -> OxyError {
    OxyError::RuntimeError(format!("compile failed: {e}"))
}
