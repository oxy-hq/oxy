//! File discovery + extension routing.
//!
//! Walks the workspace once and classifies each interesting file by
//! kind so the compiler can dispatch to the right per-kind parser.
//! Skips the obvious non-source directories (`.git`, `target`,
//! `node_modules`, `dist`, `.semantics`, `.preagg`) the same way the
//! preagg executor's walk does.

use crate::errors::CompileError;
use glob::glob;
use std::path::{Path, PathBuf};
use tracing::warn;

/// One classified file the compiler will process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Workspace-relative path (forward slashes).
    pub rel_path: String,
    /// Absolute path; what we actually open.
    pub abs_path: PathBuf,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// `config.yml` at the workspace root.
    Config,
    /// `*.agentic.yml`.
    AgenticAgent,
    /// `*.view.yml`.
    SemanticView,
    /// `*.topic.yml`.
    SemanticTopic,
    /// `*.app.yml`.
    App,
    /// `*.procedure.yml`, `*.workflow.yml`, `*.automation.yml`.
    /// The exact extension is recorded so the row carries `extension`.
    Procedure(ProcedureKind),
    /// `*.sql` outside `modeling/` (modeling SQL is dbt-shaped, handled by Airform later).
    VerifiedQuery,
    /// `*.airway.yml`.
    AirwayPipeline,
    /// `.monitor.yml` at the workspace root — the anomaly-monitor
    /// configuration. Singleton per workspace.
    MonitorConfig,
    /// `.world-model.yml` at the workspace root — the world-model entity
    /// labels / display fields / allowlist. Singleton per workspace.
    WorldModelConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProcedureKind {
    Procedure,
    Workflow,
    Automation,
}

impl ProcedureKind {
    pub fn extension(&self) -> &'static str {
        match self {
            ProcedureKind::Procedure => "procedure",
            ProcedureKind::Workflow => "workflow",
            ProcedureKind::Automation => "automation",
        }
    }
}

/// Walk the workspace and return every interesting file, classified.
/// Order is stable (sorted by `rel_path`) so the same workspace
/// always produces the same revision_id-relative order of work — makes
/// progress logs and failure summaries deterministic for replay.
pub fn discover(workspace_root: &Path) -> Result<Vec<DiscoveredFile>, CompileError> {
    if !workspace_root.exists() {
        return Err(CompileError::WorkspaceNotFound(
            workspace_root.display().to_string(),
        ));
    }
    if !workspace_root.is_dir() {
        return Err(CompileError::WorkspaceNotFound(format!(
            "{} is not a directory",
            workspace_root.display()
        )));
    }

    let mut out = Vec::new();

    // config.yml — root-only.
    let config_path = workspace_root.join("config.yml");
    if config_path.is_file() {
        out.push(DiscoveredFile {
            rel_path: "config.yml".to_string(),
            abs_path: config_path,
            kind: FileKind::Config,
        });
    }

    // .monitor.yml — root-only, anomaly monitor configuration. Optional.
    let monitor_path = workspace_root.join(".monitor.yml");
    if monitor_path.is_file() {
        out.push(DiscoveredFile {
            rel_path: ".monitor.yml".to_string(),
            abs_path: monitor_path,
            kind: FileKind::MonitorConfig,
        });
    }

    // .world-model.yml — root-only, world-model entity config. Optional.
    let world_model_path = workspace_root.join(".world-model.yml");
    if world_model_path.is_file() {
        out.push(DiscoveredFile {
            rel_path: ".world-model.yml".to_string(),
            abs_path: world_model_path,
            kind: FileKind::WorldModelConfig,
        });
    }

    // YAML kinds — one glob per extension. Done sequentially because
    // workspaces have ~tens to ~hundreds of files and the per-glob
    // overhead is negligible.
    push_glob(
        workspace_root,
        "**/*.agentic.yml",
        FileKind::AgenticAgent,
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.view.yml",
        FileKind::SemanticView,
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.topic.yml",
        FileKind::SemanticTopic,
        &mut out,
    )?;
    push_glob(workspace_root, "**/*.app.yml", FileKind::App, &mut out)?;
    push_glob(
        workspace_root,
        "**/*.procedure.yml",
        FileKind::Procedure(ProcedureKind::Procedure),
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.workflow.yml",
        FileKind::Procedure(ProcedureKind::Workflow),
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.automation.yml",
        FileKind::Procedure(ProcedureKind::Automation),
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.airway.yml",
        FileKind::AirwayPipeline,
        &mut out,
    )?;
    push_glob(
        workspace_root,
        "**/*.sql",
        FileKind::VerifiedQuery,
        &mut out,
    )?;

    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

/// Glob expansion + skip filter + classification helper.
fn push_glob(
    root: &Path,
    pattern: &str,
    kind: FileKind,
    out: &mut Vec<DiscoveredFile>,
) -> Result<(), CompileError> {
    let glob_pattern = root.join(pattern).to_string_lossy().into_owned();
    let entries = glob(&glob_pattern).map_err(|e| CompileError::Walk(e.to_string()))?;

    for entry in entries {
        let abs_path = match entry {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, pattern = pattern, "glob entry failed");
                continue;
            }
        };

        if !abs_path.is_file() {
            continue;
        }

        let rel = match abs_path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        if is_skipped(&rel) {
            continue;
        }

        // Don't include test files that mirror an extension —
        // `foo.agent.test.yml` is NOT a real `.agentic.yml`.
        // Same idea as `crates/core/src/config/storage.rs:421`.
        if rel.contains(".test.") {
            continue;
        }

        // VerifiedQuery only applies outside `modeling/` — the dbt-shaped
        // SQL files there are handled by Airform's separate compile
        // pipeline (Phase 1.6c will fold those in).
        if matches!(kind, FileKind::VerifiedQuery) && rel.starts_with("modeling/") {
            continue;
        }

        out.push(DiscoveredFile {
            rel_path: rel,
            abs_path,
            kind,
        });
    }

    Ok(())
}

/// Directories that should never be walked: build outputs, vendored
/// dependencies, VCS internals, derived artifact directories.
fn is_skipped(rel_path: &str) -> bool {
    // KEEP IN SYNC with the wider skip-set documented in
    // `product-context.md` -> 'Semantic file discovery in hidden/build dirs'.
    // Missing a prefix here matters more than for plain discovery: a stray
    // .view.yml / .topic.yml / .agentic.yml copy under .oxy_state/ or build/
    // produces a Duplicate failure, which flips the whole revision to Failed
    // and prevents promotion -- so a single intermediate artifact could break
    // every compile on the workspace.
    const SKIPPED_PREFIXES: &[&str] = &[
        ".git/",
        "target/",
        "node_modules/",
        "dist/",
        "build/",
        ".oxy_state/",
        ".semantics/",
        ".preagg/",
        ".worktrees/",
        ".cubestore/",
        ".airlayer_cache/",
    ];
    SKIPPED_PREFIXES.iter().any(|p| rel_path.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, content: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn discover_finds_all_kinds_and_skips_noise() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        write(root, "config.yml", "models: []");
        write(root, "agents/foo.agentic.yml", "name: foo");
        write(root, "semantics/views/orders.view.yml", "name: orders");
        write(root, "semantics/topics/sales.topic.yml", "name: sales");
        write(root, "apps/dash.app.yml", "name: dash");
        write(root, "workflows/x.procedure.yml", "name: x");
        write(root, "workflows/legacy.workflow.yml", "name: legacy");
        write(root, "workflows/cron.automation.yml", "name: cron");
        write(root, "pipelines/data.airway.yml", "name: data");
        write(root, "queries/top.sql", "SELECT 1");

        // noise we should skip
        write(root, ".git/config", "[core]");
        write(root, "target/debug/something", "binary");
        write(root, "node_modules/foo/index.js", "");
        write(root, ".semantics/.manifest.json", "{}");
        write(root, "tests/sales.agent.test.yml", "name: t");
        write(root, "modeling/proj/models/foo.sql", "SELECT 1");

        let found = discover(root).unwrap();
        let by_kind: Vec<_> = found.iter().map(|f| (f.rel_path.clone(), f.kind)).collect();

        // config.yml present
        assert!(
            by_kind
                .iter()
                .any(|(p, k)| p == "config.yml" && matches!(k, FileKind::Config))
        );

        // each kind present exactly once (no test/modeling noise)
        assert_eq!(
            by_kind
                .iter()
                .filter(|(_, k)| matches!(k, FileKind::AgenticAgent))
                .count(),
            1,
            "exactly one .agentic.yml (test file should be filtered)"
        );
        assert_eq!(
            by_kind
                .iter()
                .filter(|(_, k)| matches!(k, FileKind::VerifiedQuery))
                .count(),
            1,
            "exactly one .sql outside modeling/"
        );

        // outputs are sorted
        let paths: Vec<_> = by_kind.iter().map(|(p, _)| p.clone()).collect();
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "discover output must be sorted by rel_path");
    }

    #[test]
    fn discover_returns_workspace_not_found_when_dir_missing() {
        let result = discover(Path::new("/nonexistent/path/oxy-test"));
        assert!(matches!(result, Err(CompileError::WorkspaceNotFound(_))));
    }
}
