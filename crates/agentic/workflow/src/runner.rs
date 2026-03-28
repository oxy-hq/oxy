//! Concrete [`ProcedureRunner`] backed by [`WorkflowLauncher`].

use std::path::{Path, PathBuf};

use agentic_analytics::procedure::{
    ProcedureError, ProcedureOutput, ProcedureRef, ProcedureRunner, ProcedureStepResult,
};
use agentic_analytics::AnalyticsEvent;
use agentic_core::events::{Event, EventStream};
use oxy::adapters::project::manager::ProjectManager;
use oxy::checkpoint::types::RetryStrategy;
use oxy::execute::writer::NoopHandler;
use oxy_workflow::{WorkflowInput, WorkflowLauncher};

use crate::event_bridge::WorkflowEventBridge;

/// Runs `.procedure.yml` files through [`WorkflowLauncher`].
///
/// Wire this into `AnalyticsSolver::with_procedure_runner(Arc::new(runner))`.
///
/// Attach an [`EventStream`] via [`with_events`](Self::with_events) to bridge
/// workflow task-lifecycle events into the analytics event stream, giving
/// observers per-step progress during multi-step procedure execution.
///
/// Supply the procedure file paths discovered from the agent config's `context`
/// globs via [`with_procedure_files`](Self::with_procedure_files).  When set,
/// `search()` uses these paths directly instead of scanning the project
/// directory via `list_workflows()`.
pub struct OxyProcedureRunner {
    project_manager: ProjectManager,
    /// Procedure file paths resolved from `context` globs at config load time.
    /// When non-empty, `search()` uses these instead of `list_workflows()`.
    procedure_files: Vec<PathBuf>,
    /// When `Some`, workflow task events are forwarded to the analytics stream
    /// via [`WorkflowEventBridge`] instead of being dropped.
    event_tx: Option<EventStream<AnalyticsEvent>>,
}

impl OxyProcedureRunner {
    pub fn new(project_manager: ProjectManager) -> Self {
        Self {
            project_manager,
            procedure_files: Vec::new(),
            event_tx: None,
        }
    }

    /// Provide the pre-resolved procedure file paths from the agent config's
    /// `context` globs.  `search()` will use these paths directly.
    pub fn with_procedure_files(mut self, files: Vec<PathBuf>) -> Self {
        self.procedure_files = files;
        self
    }

    /// Attach an analytics event stream.
    ///
    /// When set, an internal [`WorkflowEventBridge`] is created for each
    /// `run()` call, translating workflow task events (started / finished /
    /// error) into [`AnalyticsEvent::ProcedureStepStarted`] /
    /// [`AnalyticsEvent::ProcedureStepCompleted`] on the supplied sender.
    pub fn with_events(mut self, tx: EventStream<AnalyticsEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }
}

#[async_trait::async_trait]
impl ProcedureRunner for OxyProcedureRunner {
    async fn run(&self, file_path: &Path) -> Result<ProcedureOutput, ProcedureError> {
        // Derive the human-readable procedure name and top-level task names so
        // we can emit lifecycle events that let the frontend show the full DAG
        // before any individual step events arrive.
        let procedure_name = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.strip_suffix(".procedure").unwrap_or(s).to_string())
            .unwrap_or_default();

        use agentic_analytics::ProcedureStepInfo;
        let steps: Vec<ProcedureStepInfo> = std::fs::read_to_string(file_path)
            .ok()
            .and_then(|s| serde_yaml::from_str::<ProcedureMeta>(&s).ok())
            .map(|m| {
                m.tasks
                    .into_iter()
                    .filter(|t| !t.name.is_empty())
                    .map(|t| ProcedureStepInfo {
                        name: t.name,
                        task_type: t.task_type,
                    })
                    .collect()
            })
            .unwrap_or_default();

        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(Event::Domain(AnalyticsEvent::ProcedureStarted {
                    procedure_name: procedure_name.clone(),
                    steps,
                }))
                .await;
        }

        let launcher = WorkflowLauncher::new()
            .with_project(self.project_manager.clone())
            .await
            .map_err(|e| ProcedureError(e.to_string()))?;

        let input = WorkflowInput {
            workflow_ref: file_path.to_string_lossy().to_string(),
            retry: RetryStrategy::NoRetry { variables: None },
        };

        let launch_result = match &self.event_tx {
            Some(tx) => {
                // Bridge workflow task events into the analytics event stream.
                let bridge = WorkflowEventBridge::new(tx.clone());
                launcher.launch(input, bridge, None).await
            }
            None => launcher.launch(input, NoopHandler, None).await,
        };

        if let Some(tx) = &self.event_tx {
            let _ = tx
                .send(Event::Domain(AnalyticsEvent::ProcedureCompleted {
                    procedure_name,
                    success: launch_result.is_ok(),
                    error: launch_result.as_ref().err().map(|e| e.to_string()),
                }))
                .await;
        }

        let output_container = launch_result.map_err(|e| ProcedureError(e.to_string()))?;
        let steps = extract_steps(&output_container);
        Ok(ProcedureOutput { steps })
    }

    async fn search(&self, query: &str) -> Vec<ProcedureRef> {
        let config = &self.project_manager.config_manager;
        let project_path = config.project_path().to_path_buf();

        // Use context-resolved paths when available; fall back to full project scan.
        let paths = if !self.procedure_files.is_empty() {
            self.procedure_files.clone()
        } else {
            match config.list_workflows().await {
                Ok(p) => p,
                Err(_) => return vec![],
            }
        };

        filter_procedure_paths(&paths, &project_path, query)
    }
}

// ---------------------------------------------------------------------------
// Per-step output extraction
// ---------------------------------------------------------------------------

/// Maximum number of steps to include; excess steps are silently dropped.
const MAX_PROCEDURE_STEPS: usize = 20;

/// Maximum character length for non-table (text fallback) cells.
const MAX_FALLBACK_TEXT_CHARS: usize = 2000;

/// Flatten a top-level `OutputContainer` (always a `Map` for procedures)
/// into an ordered vec of per-step results.
fn extract_steps(container: &oxy::execute::types::OutputContainer) -> Vec<ProcedureStepResult> {
    use oxy::execute::types::OutputContainer;

    match container {
        OutputContainer::Map(map) => map
            .iter()
            .filter(|(_, v)| !matches!(v, OutputContainer::Variable(_)))
            .take(MAX_PROCEDURE_STEPS)
            .map(|(name, output)| step_to_result(name.clone(), output))
            .collect(),
        other => vec![step_to_result("result".to_string(), other)],
    }
}

/// Convert one step's `OutputContainer` into a [`ProcedureStepResult`].
fn step_to_result(
    step_name: String,
    container: &oxy::execute::types::OutputContainer,
) -> ProcedureStepResult {
    use oxy::execute::types::{Output, OutputContainer};

    // Unwrap one level of Metadata / Consistency.
    let inner = match container {
        OutputContainer::Metadata { value, .. } => value.output.as_ref(),
        OutputContainer::Consistency { value, .. } => value.output.as_ref(),
        other => other,
    };

    match inner {
        OutputContainer::Single(Output::Table(table)) => {
            match (table.columns(), table.to_typed_rows()) {
                (Ok(columns), Ok((rows, truncated))) => {
                    let total_row_count = rows.len() as u64;
                    ProcedureStepResult {
                        step_name,
                        columns,
                        rows,
                        truncated,
                        total_row_count,
                    }
                }
                // Arrow file unreadable → text fallback.
                _ => text_fallback(step_name, &format!("{container}")),
            }
        }
        other => text_fallback(step_name, &format!("{other}")),
    }
}

fn text_fallback(step_name: String, text: &str) -> ProcedureStepResult {
    let is_long = text.len() > MAX_FALLBACK_TEXT_CHARS;
    let cell = if is_long {
        format!("{}…", &text[..MAX_FALLBACK_TEXT_CHARS])
    } else {
        text.to_string()
    };
    ProcedureStepResult {
        step_name,
        columns: vec!["result".to_string()],
        rows: vec![vec![serde_json::Value::String(cell)]],
        truncated: is_long,
        total_row_count: 1,
    }
}

/// Minimal YAML metadata read from each procedure file.
#[derive(serde::Deserialize, Default)]
struct ProcedureMeta {
    #[serde(default)]
    description: String,
    #[serde(default)]
    retrieval: RetrievalMeta,
    #[serde(default)]
    tasks: Vec<ProcedureTaskMeta>,
}

/// Minimal task metadata — only the name and type are needed for lifecycle events.
#[derive(serde::Deserialize, Default)]
struct ProcedureTaskMeta {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    task_type: String,
}

#[derive(serde::Deserialize, Default)]
struct RetrievalMeta {
    /// Phrases that opt this procedure *in*: if non-empty, the query must match
    /// at least one phrase, otherwise the procedure is excluded.
    #[serde(default)]
    include: Vec<String>,
    /// Phrases that opt this procedure *out*: if the query matches any phrase,
    /// the procedure is always excluded regardless of other matches.
    #[serde(default)]
    exclude: Vec<String>,
}

/// Minimum Jaro-Winkler similarity for a query token to count as a "match"
/// against a corpus word, and for include-gate phrase tokens to match query
/// tokens.  0.85 tolerates common typos and word-form variants ("store" ↔
/// "stores", "performance" ↔ "performing") while rejecting unrelated words.
const FUZZY_THRESHOLD: f64 = 0.85;

/// Returns `true` when at least one token of `phrase_lower` has a
/// Jaro-Winkler similarity ≥ [`FUZZY_THRESHOLD`] against at least one token
/// in `query_tokens`.  This makes the include gate robust to word-form
/// variation (e.g. "store analysis" matches a query containing "stores").
fn phrase_fuzzy_matches(phrase_lower: &str, query_tokens: &[&str]) -> bool {
    if query_tokens.is_empty() {
        return false;
    }
    phrase_lower.split_whitespace().any(|pt| {
        query_tokens
            .iter()
            .any(|qt| strsim::jaro_winkler(pt, qt) >= FUZZY_THRESHOLD)
    })
}

fn filter_procedure_paths(
    paths: &[PathBuf],
    project_path: &Path,
    query: &str,
) -> Vec<ProcedureRef> {
    let query_lower = query.to_lowercase();

    // Split the query into lowercase tokens; empty tokens are dropped.
    let tokens: Vec<&str> = query_lower.split_whitespace().collect();

    // Collect all procedure candidates, reading YAML metadata upfront so that
    // description and retrieval config participate in filtering and scoring.
    let mut scored: Vec<(f64, ProcedureRef)> = paths
        .iter()
        .filter(|p| p.to_string_lossy().contains(".procedure."))
        .filter_map(|path| {
            let stem = path.file_stem()?.to_str()?.to_string();
            let name = stem.strip_suffix(".procedure").unwrap_or(&stem).to_string();

            // Read and parse metadata before any filtering.
            let meta: ProcedureMeta = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_yaml::from_str(&s).ok())
                .unwrap_or_default();

            // ── retrieval config: hard include / exclude gates ──────────────
            // These mirror the same semantics used across the rest of the Oxy
            // retrieval system (vector store, agents, SQL files, topics).
            if !query_lower.is_empty() {
                // Exclude gate: exact substring match (conservative — only
                // block when the phrase is literally present in the query).
                let excluded = meta
                    .retrieval
                    .exclude
                    .iter()
                    .any(|phrase| query_lower.contains(phrase.to_lowercase().as_str()));
                if excluded {
                    return None;
                }

                // Include gate: fuzzy phrase-token matching (liberal — at
                // least one token of any include phrase must fuzzy-match a
                // query token).  This handles word-form variation such as
                // "store analysis" matching a query that says "stores".
                let includes = &meta.retrieval.include;
                if !includes.is_empty() {
                    let included = includes
                        .iter()
                        .any(|phrase| phrase_fuzzy_matches(&phrase.to_lowercase(), &tokens));
                    if !included {
                        return None;
                    }
                }
            }

            // ── fuzzy token scoring over name + description ─────────────────
            // Build corpus words by splitting on whitespace, underscores, and
            // hyphens so that e.g. "monthly_revenue" → ["monthly", "revenue"].
            // Score = sum of max Jaro-Winkler similarity per query token across
            // all corpus words.  An empty query matches everything with score 0.
            let corpus_raw = format!("{} {}", name, meta.description).to_lowercase();
            let corpus_words: Vec<&str> = corpus_raw
                .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
                .filter(|s| !s.is_empty())
                .collect();

            let token_scores: Vec<f64> = if tokens.is_empty() {
                vec![]
            } else {
                tokens
                    .iter()
                    .map(|t| {
                        corpus_words
                            .iter()
                            .map(|w| strsim::jaro_winkler(t, w))
                            .fold(0.0_f64, f64::max)
                    })
                    .collect()
            };

            let score: f64 = token_scores.iter().sum();

            // Require at least one query token to have a strong fuzzy match
            // (≥ FUZZY_THRESHOLD) so that entirely unrelated procedures are
            // excluded even if they accumulate a non-zero sum from many weak
            // partial similarities.
            if !tokens.is_empty() && !token_scores.iter().any(|&s| s >= FUZZY_THRESHOLD) {
                return None;
            }

            let abs_path = if path.is_absolute() {
                path.clone()
            } else {
                project_path.join(path)
            };

            Some((
                score,
                ProcedureRef {
                    name,
                    path: abs_path,
                    description: meta.description,
                },
            ))
        })
        .collect();

    // Sort by descending score so the most relevant procedures come first.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn make_path(name: &str) -> PathBuf {
        PathBuf::from(format!("/project/{name}"))
    }

    // NOTE: step_extraction_tests (text_fallback, extract_steps) are below;
    // the rows field is now Vec<Vec<serde_json::Value>>, so helpers there use json!().

    #[test]
    fn non_procedure_files_are_excluded() {
        let paths = vec![
            make_path("monthly_revenue.workflow.yml"),
            make_path("monthly_revenue.procedure.yml"),
        ];
        let refs = filter_procedure_paths(&paths, Path::new("/project"), "");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "monthly_revenue");
    }

    #[test]
    fn empty_query_returns_all_procedures() {
        let paths = vec![
            make_path("revenue.procedure.yml"),
            make_path("churn.procedure.yml"),
        ];
        let refs = filter_procedure_paths(&paths, Path::new("/project"), "");
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn query_filters_by_name_token() {
        let paths = vec![
            make_path("monthly_revenue.procedure.yml"),
            make_path("churn_rate.procedure.yml"),
        ];
        let refs = filter_procedure_paths(&paths, Path::new("/project"), "revenue");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "monthly_revenue");
    }

    #[test]
    fn query_is_case_insensitive() {
        let paths = vec![make_path("Monthly_Revenue.procedure.yml")];
        let refs = filter_procedure_paths(&paths, Path::new("/project"), "REVENUE");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn no_match_returns_empty_vec() {
        let paths = vec![make_path("churn_rate.procedure.yml")];
        let refs = filter_procedure_paths(&paths, Path::new("/project"), "revenue");
        assert!(refs.is_empty());
    }

    #[test]
    fn query_matches_description_not_name() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("orders.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "name: orders\ndescription: Monthly revenue breakdown").unwrap();

        // "revenue" is only in the description, not the name
        let refs = filter_procedure_paths(&[file_path], dir.path(), "revenue");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].description, "Monthly revenue breakdown");
    }

    #[test]
    fn multi_token_query_scores_and_orders_results() {
        let dir = tempfile::tempdir().unwrap();

        // "sales_summary" matches both tokens: "sales" (name) + "monthly" (description)
        let p1 = dir.path().join("sales_summary.procedure.yml");
        let mut f = std::fs::File::create(&p1).unwrap();
        writeln!(f, "name: sales_summary\ndescription: Monthly sales report").unwrap();

        // "churn_rate" matches one token: "monthly" (description only)
        let p2 = dir.path().join("churn_rate.procedure.yml");
        let mut f = std::fs::File::create(&p2).unwrap();
        writeln!(f, "name: churn_rate\ndescription: Monthly churn metrics").unwrap();

        let refs = filter_procedure_paths(&[p1, p2], dir.path(), "sales monthly");
        assert_eq!(refs.len(), 2);
        // Higher-scoring result (2 tokens matched) comes first
        assert_eq!(refs[0].name, "sales_summary");
        assert_eq!(refs[1].name, "churn_rate");
    }

    #[test]
    fn description_read_from_valid_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("orders.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "name: orders\ndescription: Order analysis").unwrap();

        let refs = filter_procedure_paths(&[file_path], dir.path(), "orders");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].description, "Order analysis");
    }

    // ── retrieval config: include / exclude gates ─────────────────────────────

    #[test]
    fn exclude_phrase_removes_procedure() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("revenue.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: revenue\ndescription: Revenue report\nretrieval:\n  exclude:\n    - apple revenue"
        )
        .unwrap();

        // Query contains the exclude phrase → procedure must not appear.
        let refs = filter_procedure_paths(&[file_path], dir.path(), "apple revenue data");
        assert!(refs.is_empty());
    }

    #[test]
    fn exclude_phrase_does_not_affect_unrelated_query() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("revenue.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: revenue\ndescription: Revenue report\nretrieval:\n  exclude:\n    - apple revenue"
        )
        .unwrap();

        // Query does NOT contain the exclude phrase → procedure appears normally.
        let refs = filter_procedure_paths(&[file_path], dir.path(), "revenue");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn include_whitelist_allows_matching_query() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fruit_sales.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: fruit_sales\ndescription: Fruit sales data\nretrieval:\n  include:\n    - fruit sales"
        )
        .unwrap();

        let refs = filter_procedure_paths(&[file_path], dir.path(), "get fruit sales data");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn include_whitelist_blocks_non_matching_query() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fruit_sales.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: fruit_sales\ndescription: Fruit sales data\nretrieval:\n  include:\n    - fruit sales"
        )
        .unwrap();

        // Query does not contain any include phrase → excluded.
        let refs = filter_procedure_paths(&[file_path], dir.path(), "revenue report");
        assert!(refs.is_empty());
    }

    #[test]
    fn empty_query_bypasses_retrieval_gates() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("fruit_sales.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: fruit_sales\ndescription: x\nretrieval:\n  include:\n    - fruit sales\n  exclude:\n    - apple"
        )
        .unwrap();

        // Empty query bypasses both gates and returns all procedures.
        let refs = filter_procedure_paths(&[file_path], dir.path(), "");
        assert_eq!(refs.len(), 1);
    }

    #[test]
    fn include_phrase_matched_as_substring_of_query() {
        // "external factors" must match as a sub-phrase of a longer query,
        // e.g. "sales correlation external factors CPI unemployment".
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir
            .path()
            .join("external-factors-correlation.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: external_factors_correlation\n\
             description: Correlation analysis\n\
             retrieval:\n  include:\n    - \"external factors\""
        )
        .unwrap();

        let refs = filter_procedure_paths(
            &[file_path],
            dir.path(),
            "sales correlation external factors CPI unemployment",
        );
        assert_eq!(refs.len(), 1, "include phrase substring should pass");
    }

    #[test]
    fn real_world_procedure_matches_external_factors_query() {
        // Mirrors the demo project's external-factors-correlation.procedure.yml
        // and the LLM query that was failing to return results.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir
            .path()
            .join("external-factors-correlation.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: external_factors_correlation\n\
             description: |\n  \
               Advanced correlation analysis between external factors (temperature, fuel prices,\n  \
               unemployment, CPI) and sales performance.\n\
             retrieval:\n  include:\n    - \"external factors\"\n    - \"unemployment analysis\""
        )
        .unwrap();

        let refs = filter_procedure_paths(
            &[file_path],
            dir.path(),
            "sales correlation external factors CPI unemployment",
        );
        assert_eq!(refs.len(), 1, "procedure should be found for this query");
        assert_eq!(refs[0].name, "external-factors-correlation");
    }

    #[test]
    fn missing_description_defaults_to_empty_string() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("orders.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(f, "name: orders\ntasks: []").unwrap();

        let refs = filter_procedure_paths(&[file_path], dir.path(), "");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].description, "");
    }

    #[test]
    fn absolute_paths_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("sales.procedure.yml");
        std::fs::File::create(&file_path).unwrap();

        let refs = filter_procedure_paths(&[file_path.clone()], dir.path(), "");
        assert_eq!(refs[0].path, file_path);
    }

    // ── fuzzy scoring ─────────────────────────────────────────────────────────

    #[test]
    fn fuzzy_query_matches_store_deep_dive_procedure() {
        // Mirrors demo_project/workflows/store-deep-dive-analysis.procedure.yml.
        // The query uses related terms ("top stores performance") that don't
        // appear verbatim in the include phrases ("store analysis", etc.) or
        // the description, but should still match via Jaro-Winkler fuzzy scoring
        // and the fuzzy include gate.
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("store-deep-dive-analysis.procedure.yml");
        let mut f = std::fs::File::create(&file_path).unwrap();
        writeln!(
            f,
            "name: store_deep_dive_analysis\n\
             description: |\n  \
               Comprehensive store-by-store analysis using loop_sequential to iterate through\n  \
               top performing stores and generate detailed individual reports for each.\n  \
               This workflow demonstrates advanced looping capabilities and nested task execution.\n\
             retrieval:\n\
               include:\n\
                 - \"store analysis\"\n\
                 - \"individual store performance\"\n\
                 - \"store deep dive\""
        )
        .unwrap();

        let refs = filter_procedure_paths(
            &[file_path],
            dir.path(),
            "top stores performance revenue sales",
        );
        assert_eq!(refs.len(), 1, "procedure should be found for this query");
        assert_eq!(refs[0].name, "store-deep-dive-analysis");
    }
}

// ---------------------------------------------------------------------------
// Tests for extract_steps / step_to_result / text_fallback
// ---------------------------------------------------------------------------

#[cfg(test)]
mod step_extraction_tests {
    use super::*;
    use oxy::execute::types::{Output, OutputContainer};

    fn text_container(s: &str) -> OutputContainer {
        OutputContainer::Single(Output::Text(s.to_string()))
    }

    fn map_of(entries: Vec<(&str, OutputContainer)>) -> OutputContainer {
        OutputContainer::Map(
            entries
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    #[test]
    fn empty_map_yields_no_steps() {
        assert!(extract_steps(&map_of(vec![])).is_empty());
    }

    #[test]
    fn variable_steps_are_filtered_out() {
        let container = map_of(vec![(
            "x",
            OutputContainer::Variable(serde_json::Value::Null),
        )]);
        assert!(extract_steps(&container).is_empty());
    }

    #[test]
    fn text_step_produces_single_cell_fallback() {
        let container = map_of(vec![("step1", text_container("hello"))]);
        let steps = extract_steps(&container);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_name, "step1");
        assert_eq!(steps[0].columns, vec!["result"]);
        assert_eq!(steps[0].rows[0][0], serde_json::json!("hello\n"));
    }

    #[test]
    fn non_map_container_wraps_as_single_result_step() {
        let steps = extract_steps(&text_container("output"));
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].step_name, "result");
    }

    #[test]
    fn step_count_is_capped_at_max() {
        let entries: Vec<_> = (0..MAX_PROCEDURE_STEPS + 5)
            .map(|i| (format!("step{i}"), text_container(&i.to_string())))
            .collect();
        let container = OutputContainer::Map(entries.into_iter().map(|(k, v)| (k, v)).collect());
        assert_eq!(extract_steps(&container).len(), MAX_PROCEDURE_STEPS);
    }

    #[test]
    fn long_text_is_truncated_in_fallback() {
        let long_text = "x".repeat(MAX_FALLBACK_TEXT_CHARS + 100);
        let step = text_fallback("s".to_string(), &long_text);
        assert!(step.truncated);
        // Cell value is a JSON string; check truncation length.
        let cell_str = step.rows[0][0].as_str().unwrap();
        assert!(cell_str.len() <= MAX_FALLBACK_TEXT_CHARS + 3); // +3 for '…'
    }

    #[test]
    fn short_text_is_not_truncated() {
        let step = text_fallback("s".to_string(), "short");
        assert!(!step.truncated);
        assert_eq!(step.rows[0][0], serde_json::json!("short"));
    }
}
