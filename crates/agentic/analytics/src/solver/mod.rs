//! Analytics domain solver — split across per-state submodules.
//!
//! Each pipeline state has its own module:
//! - [`clarifying`]  — Clarify (triage + ground)
//! - [`specifying`]  — Specify (hybrid semantic-layer + LLM, fan-out)
//! - [`solving`]     — Solve (SQL generation)
//! - [`executing`]   — Execute (connector dispatch, path-aware diagnosis)
//! - [`interpreting`]— Interpret (LLM narrative, multi-result merge)
//! - [`diagnosing`]  — Diagnose (error → recovery routing table)
//! - [`resuming`]    — HITL (ask_user, suspend/resume)
//! - [`prompts`]     — Shared prompt constants and formatting helpers

pub(crate) mod clarifying;
pub(crate) mod diagnosing;
pub(crate) mod executing;
pub(crate) mod interpreting;
pub(crate) mod prompts;
pub(crate) mod resuming;
pub(crate) mod solving;
pub(crate) mod specifying;

mod helpers;
pub(super) use helpers::{
    emit_core, emit_domain, fmt_result_shape, infer_result_shape, is_retryable_compile_error,
    strip_json_fences,
};

mod solver;
pub use solver::AnalyticsSolver;

use std::collections::HashMap;

use agentic_core::orchestrator::StateHandler;

use crate::AnalyticsDomain;
use crate::events::AnalyticsEvent;

mod builder;

mod domain_solver;

// ---------------------------------------------------------------------------
// Table-driven handlers
// ---------------------------------------------------------------------------

/// Build the analytics-specific state handler table.
///
/// Each handler overrides the generic default with analytics-aware logic:
/// - **clarifying** — delegates to `clarify_impl`; short-circuits `GeneralInquiry`.
/// - **specifying** — hybrid: semantic layer → LLM fallback; fan-out on multiple specs.
/// - **solving** — delegates to `solve_impl`; propagates `solution_source`.
/// - **executing** — path-aware diagnosis: `SemanticLayer` → Specify, `LlmWithSemanticContext` → Solve.
/// - **interpreting** — delegates to `interpret_impl`.
pub fn build_analytics_handlers()
-> HashMap<&'static str, StateHandler<AnalyticsDomain, AnalyticsSolver, AnalyticsEvent>> {
    let mut map = HashMap::new();
    map.insert("clarifying", clarifying::build_clarifying_handler());
    map.insert("specifying", specifying::build_specifying_handler());
    // Solving is absorbed into the specifying handler — no separate handler.
    map.insert("executing", executing::build_executing_handler());
    map.insert("interpreting", interpreting::build_interpreting_handler());
    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::clarifying::build_triage_user_prompt;
    use super::interpreting::build_interpret_user_prompt;
    use super::prompts::format_session_turns_section;
    use super::*;
    use crate::{
        AnalyticsError, AnalyticsIntent, AnalyticsResult, AnalyticsSolution, LlmClient, QuerySpec,
        QuestionType, ResultShape, SemanticCatalog, SolutionPayload, SolutionSource,
    };
    use agentic_connector::{ConnectorError, ExecutionResult, SqlDialect};
    use agentic_core::state::ProblemState;
    use agentic_core::{BackTarget, DomainSolver, QueryResult};
    use async_trait::async_trait;

    // ── StubConnector — always returns an error ────────────────────────────────

    struct StubConnector;

    #[async_trait]
    impl agentic_connector::DatabaseConnector for StubConnector {
        fn dialect(&self) -> SqlDialect {
            SqlDialect::DuckDb
        }

        async fn execute_query(
            &self,
            _sql: &str,
            _limit: u64,
        ) -> Result<ExecutionResult, ConnectorError> {
            Err(ConnectorError::Other("stub".into()))
        }
    }

    // ── Fixtures ──────────────────────────────────────────────────────────────

    fn make_solver() -> AnalyticsSolver {
        AnalyticsSolver::new(
            LlmClient::new("dummy"),
            SemanticCatalog::empty(),
            Box::new(StubConnector),
        )
    }

    /// Build a SemanticCatalog with orders + customers views for tests
    /// that need table names in prompts.
    fn make_semantic_catalog_with_tables() -> SemanticCatalog {
        use crate::airlayer_compat;
        let orders_yaml = r#"
name: orders
description: Orders
table: orders
dimensions:
  - name: order_id
    type: number
    expr: order_id
  - name: revenue
    type: number
    expr: revenue
  - name: date
    type: date
    expr: date
"#;
        let customers_yaml = r#"
name: customers
description: Customers
table: customers
dimensions:
  - name: customer_id
    type: number
    expr: customer_id
  - name: region
    type: string
    expr: region
"#;
        let views = vec![
            airlayer_compat::parse_view_yaml(orders_yaml).unwrap(),
            airlayer_compat::parse_view_yaml(customers_yaml).unwrap(),
        ];
        let layer = airlayer::SemanticLayer::new(views, None);
        let dialects = airlayer::DatasourceDialectMap::with_default(airlayer::Dialect::DuckDB);
        let engine = airlayer::SemanticEngine::from_semantic_layer(layer, dialects).unwrap();
        SemanticCatalog::from_engine(engine)
    }

    fn make_intent() -> AnalyticsIntent {
        AnalyticsIntent {
            raw_question: "What is total revenue by region?".into(),
            summary: "Total revenue broken down by region".into(),
            question_type: QuestionType::Breakdown,
            metrics: vec!["revenue".into()],
            dimensions: vec!["region".into()],
            filters: vec![],
            history: vec![],
            spec_hint: None,
            selected_procedure: None,
            semantic_query: Default::default(),
            semantic_confidence: 0.0,
        }
    }

    fn make_spec() -> QuerySpec {
        QuerySpec {
            intent: make_intent(),
            resolved_metrics: vec!["orders.revenue".into()],
            resolved_filters: vec![],
            resolved_tables: vec!["orders".into(), "customers".into()],
            join_path: vec![("orders".into(), "customers".into(), "customer_id".into())],
            expected_result_shape: ResultShape::Table {
                columns: vec!["region".into(), "revenue".into()],
            },
            assumptions: vec![],
            solution_source: Default::default(),
            precomputed: None,
            context: None,
            connector_name: "default".to_string(),
            query_request_item: None,
            query_request: None,
            compile_error: None,
        }
    }

    fn make_solution() -> AnalyticsSolution {
        AnalyticsSolution {
            payload: SolutionPayload::Sql(String::new()),
            solution_source: Default::default(),
            connector_name: "default".to_string(),
        }
    }

    fn make_result() -> AnalyticsResult {
        AnalyticsResult::single(
            QueryResult {
                columns: vec![],
                rows: vec![],
                total_row_count: 0,
                truncated: false,
            },
            None,
        )
    }

    fn make_run_ctx() -> agentic_core::orchestrator::RunContext<AnalyticsDomain> {
        agentic_core::orchestrator::RunContext {
            intent: None,
            spec: None,
            retry_ctx: None,
        }
    }

    /// RunContext pre-populated with an intent — used by tests that simulate
    /// recovery from a `BackTarget::Solve` back-edge, where the intent is no
    /// longer embedded in the spec (HasIntent removed) but is available in ctx.
    fn make_run_ctx_with_intent() -> agentic_core::orchestrator::RunContext<AnalyticsDomain> {
        agentic_core::orchestrator::RunContext {
            intent: Some(make_intent()),
            spec: None,
            retry_ctx: None,
        }
    }

    // ── NeedsUserInput → always fatal ─────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_needs_user_input_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::NeedsUserInput {
            prompt: "Which table?".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Clarify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::NeedsUserInput { .. })
        ));
    }

    // ── AmbiguousColumn → Clarifying ─────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_ambiguous_column_with_clarify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::AmbiguousColumn {
            column: "customer_id".into(),
            tables: vec!["orders".into(), "customers".into()],
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Clarify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Clarifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_ambiguous_column_with_specify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::AmbiguousColumn {
            column: "customer_id".into(),
            tables: vec!["orders".into(), "customers".into()],
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Specify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Clarifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_ambiguous_column_with_solve_back_uses_ctx_intent() {
        let mut s = make_solver();
        let err = AnalyticsError::AmbiguousColumn {
            column: "customer_id".into(),
            tables: vec!["orders".into(), "customers".into()],
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx_with_intent()
            )
            .await,
            Ok(ProblemState::Clarifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_ambiguous_column_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::AmbiguousColumn {
            column: "x".into(),
            tables: vec!["a".into(), "b".into()],
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::AmbiguousColumn { .. })
        ));
    }

    // ── UnresolvedMetric → Specifying ─────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_unresolved_metric_with_specify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::UnresolvedMetric {
            metric: "revenue".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Specify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_unresolved_metric_with_solve_back_uses_ctx_intent() {
        let mut s = make_solver();
        let err = AnalyticsError::UnresolvedMetric {
            metric: "revenue".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx_with_intent()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_unresolved_metric_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::UnresolvedMetric {
            metric: "revenue".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::UnresolvedMetric { .. })
        ));
    }

    // ── SyntaxError ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_syntax_error_with_solve_back() {
        let mut s = make_solver();
        let err = AnalyticsError::SyntaxError {
            query: "SELECT * FORM orders".into(),
            message: "unexpected token".into(),
        };
        // Solving is absorbed into specifying — BackTarget::Solve routes to Specifying.
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_syntax_error_with_specify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::SyntaxError {
            query: "bad sql".into(),
            message: "parse error".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Specify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_syntax_error_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::SyntaxError {
            query: "bad".into(),
            message: "error".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::SyntaxError { .. })
        ));
    }

    // ── EmptyResults → Specifying ─────────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_empty_results_with_solve_back() {
        let mut s = make_solver();
        let err = AnalyticsError::EmptyResults {
            query: "SELECT …".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx_with_intent()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_empty_results_with_clarify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::EmptyResults {
            query: "SELECT …".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Clarify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_empty_results_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::EmptyResults {
            query: "SELECT …".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::EmptyResults { .. })
        ));
    }

    // ── ShapeMismatch ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_shape_mismatch_with_solve_back() {
        let mut s = make_solver();
        let err = AnalyticsError::ShapeMismatch {
            expected: ResultShape::Scalar,
            actual: ResultShape::Series,
        };
        // Solving is absorbed into specifying — BackTarget::Solve routes to Specifying.
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_shape_mismatch_with_specify_back() {
        let mut s = make_solver();
        let err = AnalyticsError::ShapeMismatch {
            expected: ResultShape::Scalar,
            actual: ResultShape::Series,
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Specify(make_intent(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Specifying(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_shape_mismatch_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::ShapeMismatch {
            expected: ResultShape::Scalar,
            actual: ResultShape::Series,
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::ShapeMismatch { .. })
        ));
    }

    // ── ValueAnomaly → Interpreting ───────────────────────────────────────────

    #[tokio::test]
    async fn diagnose_value_anomaly_with_interpret_back() {
        let mut s = make_solver();
        let err = AnalyticsError::ValueAnomaly {
            column: "revenue".into(),
            value: "999999".into(),
            reason: "outlier".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Interpret(make_result(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Ok(ProblemState::Interpreting(_))
        ));
    }

    #[tokio::test]
    async fn diagnose_value_anomaly_with_execute_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::ValueAnomaly {
            column: "revenue".into(),
            value: "NaN".into(),
            reason: "nan detected".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Execute(make_solution(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::ValueAnomaly { .. })
        ));
    }

    #[tokio::test]
    async fn diagnose_value_anomaly_with_solve_back_is_fatal() {
        let mut s = make_solver();
        let err = AnalyticsError::ValueAnomaly {
            column: "revenue".into(),
            value: "Inf".into(),
            reason: "infinite".into(),
        };
        assert!(matches!(
            s.diagnose(
                err,
                BackTarget::Solve(make_spec(), Default::default()),
                &make_run_ctx()
            )
            .await,
            Err(AnalyticsError::ValueAnomaly { .. })
        ));
    }

    // should_skip tests removed — solving is absorbed into specifying.

    // ── Procedure path ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn specify_impl_short_circuits_when_selected_procedure_is_set() {
        let mut s = make_solver();
        let file_path = std::path::PathBuf::from("workflows/monthly_sales.procedure.yml");
        let intent = AnalyticsIntent {
            selected_procedure: Some(file_path.clone()),
            ..make_intent()
        };

        let result = s.specify_impl(intent, None).await;

        let specs = result
            .map_err(|(e, _)| e)
            .expect("specify_impl must succeed when selected_procedure is set");
        assert_eq!(specs.len(), 1, "must return exactly one spec");
        assert_eq!(
            specs[0].solution_source,
            SolutionSource::Procedure {
                file_path: file_path.clone()
            },
            "spec must carry SolutionSource::Procedure with the selected path",
        );
        assert!(
            specs[0].resolved_metrics.is_empty(),
            "short-circuit spec must have no resolved metrics (LLM was not called)",
        );
    }

    /// Regression: `build_specifying_handler` unconditionally overwrites
    /// `solution_source` to `LlmWithSemanticContext` after `specify_impl` returns,
    /// even when `specify_impl` short-circuited with `SolutionSource::Procedure`.
    ///
    /// This means that even when `intent.selected_procedure` is set, the spec
    /// forwarded to the Solving stage has the wrong `solution_source`, causing
    /// `should_skip` to miss the procedure path and `SpecResolved` to emit `"Llm"`.
    #[tokio::test]
    async fn specifying_handler_preserves_procedure_solution_source_when_selected_procedure_is_set()
    {
        let file_path = std::path::PathBuf::from("workflows/monthly_sales.procedure.yml");
        let intent = AnalyticsIntent {
            selected_procedure: Some(file_path.clone()),
            ..make_intent()
        };

        let mut solver = make_solver();
        let handlers = build_analytics_handlers();
        let execute_fn = {
            let h = handlers
                .get("specifying")
                .expect("specifying handler must exist");
            std::sync::Arc::clone(&h.execute)
        };
        let run_ctx = make_run_ctx();
        let memory = agentic_core::orchestrator::SessionMemory::new(0);

        let result = execute_fn(
            &mut solver,
            ProblemState::Specifying(intent),
            &None,
            &run_ctx,
            &memory,
        )
        .await;

        // Specifying handler now transitions directly to Executing for procedures.
        match result.state_data {
            ProblemState::Executing(solution) => {
                assert_eq!(
                    solution.solution_source,
                    SolutionSource::Procedure {
                        file_path: file_path.clone()
                    },
                    "specifying handler must preserve SolutionSource::Procedure when \
                     selected_procedure is set — got {:?} instead",
                    solution.solution_source,
                );
            }
            other => panic!(
                "expected ProblemState::Executing, got: {:?}",
                std::mem::discriminant(&other)
            ),
        }
    }

    // ── Prompt builder: session turns injection ───────────────────────────────

    fn make_completed_turn(
        question: &str,
        answer: &str,
    ) -> agentic_core::CompletedTurn<AnalyticsDomain> {
        agentic_core::CompletedTurn {
            intent: AnalyticsIntent {
                raw_question: question.into(),
                summary: String::new(),
                question_type: QuestionType::Breakdown,
                metrics: vec![],
                dimensions: vec![],
                filters: vec![],
                history: vec![],
                spec_hint: None,
                selected_procedure: None,
                semantic_query: Default::default(),
                semantic_confidence: 0.0,
            },
            spec: None,
            answer: crate::AnalyticsAnswer {
                text: answer.into(),
                display_blocks: vec![],
                spec_hint: None,
            },
            trace_id: "t-test".into(),
        }
    }

    fn make_hypothesis() -> crate::types::DomainHypothesis {
        crate::types::DomainHypothesis {
            summary: "The user wants revenue broken down by region.".into(),
            question_type: QuestionType::Breakdown,
            time_scope: None,
            confidence: 0.9,
            ambiguities: vec![],
            ambiguity_questions: vec![],
            selected_procedure_path: None,
            semantic_query: None,
            semantic_confidence: 0.0,
        }
    }

    #[test]
    fn triage_prompt_includes_question() {
        let intent = make_intent();
        let prompt = build_triage_user_prompt(&intent, &[], "");
        assert!(
            prompt.contains("total revenue"),
            "should contain the raw question"
        );
        assert!(
            prompt.contains("search_procedures"),
            "should instruct to call search_procedures"
        );
        assert!(
            prompt.contains("search_catalog"),
            "should instruct to use search_catalog for discovery"
        );
    }

    #[test]
    fn interpret_prompt_includes_last_turn_for_comparison() {
        let prior = vec![
            make_completed_turn("First question", "First answer"),
            make_completed_turn("What is revenue by region?", "West leads."),
        ];
        let result = make_result();
        let prompt = build_interpret_user_prompt(
            "How about by product category?",
            &[],
            &result,
            None,
            &prior,
            None,
        );
        assert!(
            prompt.contains("Previous question:"),
            "should include prior question label"
        );
        assert!(
            prompt.contains("What is revenue by region?"),
            "should include last prior question"
        );
        assert!(
            prompt.contains("West leads."),
            "should include last prior answer"
        );
        assert!(
            prompt.contains("comparatively"),
            "should mention comparative framing"
        );
    }

    #[test]
    fn interpret_prompt_without_session_history_has_no_prior_question() {
        let result = make_result();
        let prompt = build_interpret_user_prompt("What is revenue?", &[], &result, None, &[], None);
        assert!(
            !prompt.contains("Previous question:"),
            "no prior question section when no prior turns",
        );
    }

    #[test]
    fn session_turns_section_formats_multiple_turns() {
        let turns = vec![
            make_completed_turn("Q1", "A1"),
            make_completed_turn("Q2", "A2"),
        ];
        let section = format_session_turns_section(&turns);
        assert!(section.contains("Turn 1:"));
        assert!(section.contains("Turn 2:"));
        assert!(section.contains("Q1"));
        assert!(section.contains("A2"));
    }

    #[test]
    fn session_turns_section_empty_for_no_turns() {
        assert_eq!(format_session_turns_section(&[]), "");
    }

    // ── strip_json_fences ─────────────────────────────────────────────────────

    #[test]
    fn strip_fences_removes_json_code_fence() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_json_fences(raw), "{\"key\": \"value\"}");
    }

    #[test]
    fn strip_fences_removes_plain_code_fence() {
        let raw = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(strip_json_fences(raw), "{\"key\": \"value\"}");
    }

    #[test]
    fn strip_fences_bare_json_is_unchanged() {
        let raw = "{\"question_type\": \"Trend\"}";
        assert_eq!(strip_json_fences(raw), raw);
    }

    #[test]
    fn strip_fences_trims_surrounding_whitespace() {
        let raw = "  \n  {\"a\": 1}  \n  ";
        assert_eq!(strip_json_fences(raw), "{\"a\": 1}");
    }

    #[test]
    fn strip_fences_json_fence_with_extra_whitespace() {
        let raw = "  ```json\n  SELECT 1\n  ```  ";
        assert_eq!(strip_json_fences(raw), "SELECT 1");
    }

    #[test]
    fn strip_fences_empty_string() {
        assert_eq!(strip_json_fences(""), "");
    }

    #[test]
    fn strip_fences_partial_fence_not_stripped() {
        let raw = "```json\n{\"a\":1}";
        let result = strip_json_fences(raw);
        assert!(
            result.contains("{\"a\":1}"),
            "JSON content must be preserved: '{result}'"
        );
    }

    // ── Budget-exhaustion suspension / resume tests ───────────────────────────

    /// Minimal Anthropic-style mock provider used in budget-exhaustion tests.
    ///
    /// Returns one pre-scripted `Vec<Chunk>` per `stream()` call and records
    /// the `messages` array sent on each call so tests can assert the resume
    /// conversation structure.
    struct ScriptedProvider {
        rounds: std::sync::Mutex<std::collections::VecDeque<Vec<crate::llm::Chunk>>>,
        captured: std::sync::Arc<std::sync::Mutex<Vec<Vec<serde_json::Value>>>>,
    }

    impl ScriptedProvider {
        fn new(
            rounds: Vec<Vec<crate::llm::Chunk>>,
            captured: std::sync::Arc<std::sync::Mutex<Vec<Vec<serde_json::Value>>>>,
        ) -> Self {
            Self {
                rounds: std::sync::Mutex::new(rounds.into()),
                captured,
            }
        }
    }

    #[async_trait]
    impl crate::llm::LlmProvider for ScriptedProvider {
        async fn stream(
            &self,
            _system: &str,
            messages: &[serde_json::Value],
            _tools: &[agentic_core::tools::ToolDef],
            _thinking: &crate::llm::ThinkingConfig,
            _response_schema: Option<&crate::llm::ResponseSchema>,
            _max_tokens_override: Option<u32>,
        ) -> Result<
            std::pin::Pin<
                Box<
                    dyn futures_core::Stream<Item = Result<crate::llm::Chunk, crate::llm::LlmError>>
                        + Send,
                >,
            >,
            crate::llm::LlmError,
        > {
            self.captured.lock().unwrap().push(messages.to_vec());
            let chunks = self.rounds.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(tokio_stream::iter(
                chunks.into_iter().map(Ok::<_, crate::llm::LlmError>),
            )))
        }

        fn assistant_message(&self, blocks: &[crate::llm::ContentBlock]) -> serde_json::Value {
            let content: Vec<serde_json::Value> = blocks
                .iter()
                .map(|b| match b {
                    crate::llm::ContentBlock::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    crate::llm::ContentBlock::ToolUse {
                        id, name, input, ..
                    } => {
                        serde_json::json!({"type":"tool_use","id":id,"name":name,"input":input})
                    }
                    other => serde_json::to_value(format!("{other:?}")).unwrap(),
                })
                .collect();
            serde_json::json!({"role": "assistant", "content": content})
        }

        fn tool_result_messages(
            &self,
            results: &[(String, String, bool)],
        ) -> Vec<serde_json::Value> {
            let blocks: Vec<serde_json::Value> = results
                .iter()
                .map(|(id, content, is_error)| {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": id,
                        "content": content,
                        "is_error": is_error
                    })
                })
                .collect();
            vec![serde_json::json!({"role": "user", "content": blocks})]
        }

        fn model_name(&self) -> &str {
            "scripted"
        }
    }

    fn make_solver_with_provider(
        provider: impl crate::llm::LlmProvider + 'static,
    ) -> AnalyticsSolver {
        AnalyticsSolver::new(
            LlmClient::with_provider(provider),
            SemanticCatalog::empty(),
            Box::new(StubConnector),
        )
    }

    /// Helper: a `Chunk` sequence that emits some text then hits MaxTokens.
    fn chunks_text_then_max_tokens(text: &str) -> Vec<crate::llm::Chunk> {
        vec![
            crate::llm::Chunk::Text(text.to_string()),
            crate::llm::Chunk::Done(crate::llm::Usage {
                input_tokens: 10,
                output_tokens: 4096,
                stop_reason: crate::llm::StopReason::MaxTokens,
                ..Default::default()
            }),
        ]
    }

    /// Helper: a `Chunk` sequence that emits a single tool call (triggers rounds).
    fn chunks_tool_call(id: &str, name: &str) -> Vec<crate::llm::Chunk> {
        vec![
            crate::llm::Chunk::ToolCall(crate::llm::ToolCallChunk {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
                provider_data: None,
            }),
            crate::llm::Chunk::Done(crate::llm::Usage {
                ..Default::default()
            }),
        ]
    }

    // -- Suspension: interpret_impl MaxToolRounds -----------------------------

    /// `interpret_impl` must suspend with `suspension_type = "max_tool_rounds"`
    /// when all tool rounds are consumed, instead of propagating a generic error.
    ///
    /// Bug: before the fix, the `.map_err` in `interpret_impl` swallowed
    /// `MaxToolRoundsReached` and returned a plain error; the user was never
    /// asked whether they want to continue with more rounds.
    #[tokio::test]
    async fn interpret_impl_max_tool_rounds_stores_suspension_data() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        // max_rounds defaults to 2; provide 3 rounds of tool calls so the
        // limit is hit on round index 2.
        let rounds: Vec<Vec<crate::llm::Chunk>> = (0..=2)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "render_chart"))
            .collect();
        let provider = ScriptedProvider::new(rounds, captured);
        let mut solver = make_solver_with_provider(provider);

        let result = make_result();
        let outcome = solver
            .interpret_impl("What is revenue?", &[], result, &[], None)
            .await;

        assert!(
            matches!(
                outcome,
                Err((
                    AnalyticsError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "interpret_impl must suspend on MaxToolRoundsReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "interpreting");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds"),
            "stage_data must tag suspension_type"
        );
        assert!(
            sd.stage_data["extra_rounds"].as_u64().is_some(),
            "extra_rounds must be stored"
        );
        assert!(
            sd.stage_data["conversation_history"].is_array(),
            "conversation_history must be stored for resume"
        );
    }

    // -- Suspension: specify_impl MaxTokens -----------------------------------

    /// `specify_impl` must suspend with `suspension_type = "max_tokens"` and
    /// store the intent so that resume can re-enter `ProblemState::Specifying`.
    #[tokio::test]
    async fn specify_impl_max_tokens_stores_suspension_data() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let provider =
            ScriptedProvider::new(vec![chunks_text_then_max_tokens("partial spec")], captured);
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        let result = solver.specify_impl(intent, None).await;

        assert!(
            matches!(
                result,
                Err((
                    AnalyticsError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "specify_impl must suspend on MaxTokensReached"
        );

        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(sd.from_state, "specifying");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );
        assert!(
            sd.stage_data["intent"].is_object(),
            "intent must be stored for resume routing"
        );
        let doubled = sd.stage_data["max_tokens_override"].as_u64().unwrap_or(0);
        assert_eq!(doubled, 8192, "must double DEFAULT_MAX_TOKENS (4096→8192)");
    }

    // -- Suspension: solve_impl MaxTokens ------------------------------------

    /// `solve_impl` must suspend with `suspension_type = "max_tokens"` and
    /// store the QuerySpec so that resume can re-enter `ProblemState::Solving`.
    #[tokio::test]
    async fn solve_impl_max_tokens_stores_suspension_data() {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let provider =
            ScriptedProvider::new(vec![chunks_text_then_max_tokens("partial sql")], captured);
        let mut solver = make_solver_with_provider(provider);

        let spec = make_spec();
        let result = solver.solve_impl(spec, None).await;

        assert!(
            matches!(
                result,
                Err((
                    AnalyticsError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "solve_impl must suspend on MaxTokensReached"
        );

        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(sd.from_state, "solving");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );
        assert!(
            sd.stage_data["spec"].is_object(),
            "QuerySpec must be stored for resume routing"
        );
    }

    // -- Resume routing: problem_state_from_resume ----------------------------

    /// Resuming from a "solving" suspension now routes to `ProblemState::Specifying`
    /// (solving is absorbed into specifying).
    #[test]
    fn problem_state_from_resume_solving_reconstructs_as_specifying() {
        use crate::solver::resuming::problem_state_from_resume;
        use agentic_core::human_input::SuspendedRunData;

        let spec = make_spec();
        let spec_value = serde_json::to_value(&spec).expect("QuerySpec must serialize");
        let data = SuspendedRunData {
            from_state: "solving".to_string(),
            original_input: "test question".to_string(),
            trace_id: String::new(),
            stage_data: serde_json::json!({
                "spec": spec_value,
                "conversation_history": [],
                "suspension_type": "max_tokens",
            }),
            question: "Continue?".to_string(),
            suggestions: vec![],
        };

        let state = problem_state_from_resume(&data);
        match state {
            ProblemState::Specifying(recovered_intent) => {
                assert_eq!(recovered_intent.raw_question, spec.intent.raw_question);
            }
            other => panic!("expected ProblemState::Specifying, got a different variant"),
        }
    }

    /// Resuming from "solving" with a corrupt spec falls back to Clarifying.
    #[test]
    fn problem_state_from_resume_solving_bad_spec_falls_back_to_clarifying() {
        use crate::solver::resuming::problem_state_from_resume;
        use agentic_core::human_input::SuspendedRunData;

        let data = SuspendedRunData {
            from_state: "solving".to_string(),
            original_input: "original question".to_string(),
            trace_id: String::new(),
            stage_data: serde_json::json!({ "spec": "not-an-object" }),
            question: String::new(),
            suggestions: vec![],
        };

        let state = problem_state_from_resume(&data);
        assert!(
            matches!(state, ProblemState::Clarifying(_)),
            "corrupt spec must fall back to Clarifying"
        );
    }

    // ── DuckDB binder-error back-edge regression ──────────────────────────────

    #[cfg(feature = "duckdb-test")]
    mod duckdb_integration {
        use super::*;
        use agentic_connector::{
            ConnectorError, DatabaseConnector, ExecutionResult, ResultSummary,
        };
        use agentic_core::back_target::BackTarget;
        use agentic_core::state::ProblemState;
        use agentic_core::{RunContext, SessionMemory};
        use duckdb::Connection;

        struct RawDuckDbConnector(std::sync::Mutex<Connection>);

        #[async_trait]
        impl DatabaseConnector for RawDuckDbConnector {
            async fn execute_query(
                &self,
                sql: &str,
                limit: u64,
            ) -> Result<ExecutionResult, ConnectorError> {
                let conn = self.0.lock().unwrap();
                conn.execute_batch(&format!(
                    "CREATE OR REPLACE TEMP TABLE _agentic_tmp AS ({sql})"
                ))
                .map_err(|e| ConnectorError::QueryFailed {
                    sql: sql.to_string(),
                    message: e.to_string(),
                })?;

                let total: u64 = conn
                    .query_row("SELECT COUNT(*) FROM _agentic_tmp", [], |r| r.get(0))
                    .map_err(|e| ConnectorError::Other(e.to_string()))?;

                let mut stmt = conn
                    .prepare(&format!("SELECT * FROM _agentic_tmp LIMIT {limit}"))
                    .map_err(|e| ConnectorError::Other(e.to_string()))?;

                let col_names: Vec<String> = stmt.column_names();
                let n_cols = col_names.len();
                let rows: Vec<agentic_core::result::QueryRow> = stmt
                    .query_map([], |row: &duckdb::Row<'_>| {
                        let cells = (0..n_cols)
                            .map(
                                |i| -> Result<agentic_core::result::CellValue, duckdb::Error> {
                                    let v: duckdb::types::Value = row.get(i)?;
                                    Ok(agentic_core::result::CellValue::Text(format!("{v:?}")))
                                },
                            )
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(agentic_core::result::QueryRow(cells))
                    })
                    .map_err(|e| ConnectorError::Other(e.to_string()))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| ConnectorError::Other(e.to_string()))?;

                let _ = conn.execute_batch("DROP TABLE IF EXISTS _agentic_tmp");

                Ok(ExecutionResult {
                    result: agentic_core::result::QueryResult {
                        columns: col_names,
                        rows,
                        total_row_count: total,
                        truncated: total > limit,
                    },
                    summary: ResultSummary {
                        row_count: total,
                        columns: vec![],
                    },
                })
            }
        }

        fn make_duckdb_connector() -> RawDuckDbConnector {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE strength (
                    "Date"       TIMESTAMP,
                    "Start Time" VARCHAR,
                    "Exercise"   VARCHAR
                );
                INSERT INTO strength VALUES
                    (TIMESTAMP '2025-01-01 00:00:00', '07:30',  'Squat'),
                    (TIMESTAMP '2025-01-02 00:00:00', NULL,     'Press'),
                    (TIMESTAMP '2025-01-03 00:00:00', '08:00',  'Row');
                "#,
            )
            .unwrap();
            RawDuckDbConnector(std::sync::Mutex::new(conn))
        }

        const BINDER_ERROR_SQL: &str = r#"
WITH strength_sessions AS (
  SELECT
    "Date" AS activity_date,
    COUNT(DISTINCT COALESCE("Start Time", CAST("Date" AS TIMESTAMP))) AS session_count
  FROM strength
  WHERE "Date" >= CURRENT_DATE - INTERVAL '8 weeks'
  GROUP BY "Date"
)
SELECT activity_date, 'strength_session_count' AS metric, session_count
FROM strength_sessions
ORDER BY activity_date ASC
"#;

        #[tokio::test]
        async fn execute_method_returns_syntax_error_with_execute_back_target() {
            let mut solver = AnalyticsSolver::new(
                LlmClient::new("dummy"),
                SemanticCatalog::empty(),
                Box::new(make_duckdb_connector()),
            );
            let solution = AnalyticsSolution {
                sql: BINDER_ERROR_SQL.to_string(),
                solution_source: SolutionSource::LlmWithSemanticContext,
                connector_name: "default".to_string(),
            };
            let result = solver.execute(solution).await;
            assert!(
                matches!(
                    result,
                    Err((
                        AnalyticsError::SyntaxError { .. },
                        BackTarget::Execute(_, _)
                    ))
                ),
                "execute() must return SyntaxError with BackTarget::Execute for a DuckDB binder error",
            );
        }

        #[tokio::test]
        async fn execute_handler_routes_binder_error_to_solving_not_fatal() {
            let mut solver = AnalyticsSolver::new(
                LlmClient::new("dummy"),
                SemanticCatalog::empty(),
                Box::new(make_duckdb_connector()),
            );
            let spec = make_spec();
            let solution = AnalyticsSolution {
                sql: BINDER_ERROR_SQL.to_string(),
                solution_source: SolutionSource::LlmWithSemanticContext,
                connector_name: "default".to_string(),
            };
            let handlers = build_analytics_handlers();
            let execute_fn = {
                let h = handlers
                    .get("executing")
                    .expect("executing handler must exist");
                Arc::clone(&h.execute)
            };
            let run_ctx = RunContext {
                intent: Some(make_intent()),
                spec: Some(spec),
                retry_ctx: None,
            };
            let memory = SessionMemory::new(0);
            let result = execute_fn(
                &mut solver,
                ProblemState::Executing(solution),
                &None,
                &run_ctx,
                &memory,
            )
            .await;
            assert!(
                matches!(result.errors, Some(ref e) if e.is_empty()),
                "execute handler must use the empty-sentinel Diagnosing path on DB error",
            );
            match result.state_data {
                ProblemState::Diagnosing { error, back } => {
                    assert!(
                        matches!(error, AnalyticsError::SyntaxError { .. }),
                        "error must be SyntaxError, got: {error:?}",
                    );
                    assert!(
                        matches!(back, BackTarget::Solve(_, _)),
                        "back must be BackTarget::Solve so diagnose can route to Solving",
                    );
                }
                _other => panic!("execute handler must produce Diagnosing on DB error"),
            }
        }
    }

    // ── VendorEngine routing tests ────────────────────────────────────────────

    /// Minimal mock engine: `translate()` always succeeds; `execute()` returns
    /// a single row with column `"n"` = 42.
    struct OkEngine;

    #[async_trait]
    impl crate::engine::SemanticEngine for OkEngine {
        fn vendor_name(&self) -> &str {
            "test_vendor"
        }

        fn translate(
            &self,
            _ctx: &crate::engine::TranslationContext,
            _intent: &AnalyticsIntent,
        ) -> Result<crate::engine::VendorQuery, crate::engine::EngineError> {
            Ok(crate::engine::VendorQuery {
                payload: serde_json::json!({ "measures": ["orders.revenue"] }),
            })
        }

        async fn ping(&self) -> Result<(), crate::engine::EngineError> {
            Ok(())
        }

        async fn execute(
            &self,
            _query: &crate::engine::VendorQuery,
        ) -> Result<agentic_core::result::QueryResult, crate::engine::EngineError> {
            use agentic_core::result::{CellValue, QueryRow};
            Ok(QueryResult {
                columns: vec!["n".to_string()],
                rows: vec![QueryRow(vec![CellValue::Number(42.0)])],
                total_row_count: 1,
                truncated: false,
            })
        }
    }

    /// Engine whose `translate()` always returns `TranslationFailed`.
    struct FailEngine;

    #[async_trait]
    impl crate::engine::SemanticEngine for FailEngine {
        fn vendor_name(&self) -> &str {
            "fail_vendor"
        }

        fn translate(
            &self,
            _ctx: &crate::engine::TranslationContext,
            _intent: &AnalyticsIntent,
        ) -> Result<crate::engine::VendorQuery, crate::engine::EngineError> {
            Err(crate::engine::EngineError::TranslationFailed(
                "unsupported query".into(),
            ))
        }

        async fn ping(&self) -> Result<(), crate::engine::EngineError> {
            Ok(())
        }

        async fn execute(
            &self,
            _query: &crate::engine::VendorQuery,
        ) -> Result<agentic_core::result::QueryResult, crate::engine::EngineError> {
            unreachable!("FailEngine::execute should never be called")
        }
    }

    // should_skip tests for vendor engine removed — solving is absorbed into specifying.

    #[test]
    fn solver_with_no_engine_has_none_engine_field() {
        let s = make_solver();
        assert!(
            s.engine.is_none(),
            "default solver must not have an engine set"
        );
    }

    // Test 9: execute_solution dispatches Vendor payload to the engine, not the SQL connector.
    #[tokio::test]
    async fn execute_solution_vendor_payload_dispatched_to_engine() {
        let mut s = AnalyticsSolver::new(
            LlmClient::new("dummy"),
            SemanticCatalog::empty(),
            Box::new(StubConnector),
        )
        .with_engine(std::sync::Arc::new(OkEngine));

        let vq = crate::engine::VendorQuery {
            payload: serde_json::json!({ "measures": ["orders.revenue"] }),
        };
        let solution = AnalyticsSolution {
            payload: SolutionPayload::Vendor(vq),
            solution_source: SolutionSource::VendorEngine("test_vendor".to_string()),
            connector_name: "default".to_string(),
        };

        let result = s
            .execute_solution(solution)
            .await
            .map_err(|(e, _)| e)
            .expect("execute should succeed");
        let primary = result.primary();
        assert_eq!(primary.data.columns, vec!["n"]);
        assert_eq!(primary.data.total_row_count, 1);
    }

    // Test that a Vendor payload error maps to AnalyticsError::VendorError.
    #[tokio::test]
    async fn execute_solution_vendor_api_error_maps_to_vendor_error() {
        struct ErrorEngine;
        #[async_trait]
        impl crate::engine::SemanticEngine for ErrorEngine {
            fn vendor_name(&self) -> &str {
                "error_vendor"
            }
            fn translate(
                &self,
                _: &crate::engine::TranslationContext,
                _: &AnalyticsIntent,
            ) -> Result<crate::engine::VendorQuery, crate::engine::EngineError> {
                unreachable!()
            }
            async fn ping(&self) -> Result<(), crate::engine::EngineError> {
                Ok(())
            }
            async fn execute(
                &self,
                _: &crate::engine::VendorQuery,
            ) -> Result<agentic_core::result::QueryResult, crate::engine::EngineError> {
                Err(crate::engine::EngineError::ApiError {
                    status: 400,
                    body: "bad request".into(),
                })
            }
        }

        let mut s = AnalyticsSolver::new(
            LlmClient::new("dummy"),
            SemanticCatalog::empty(),
            Box::new(StubConnector),
        )
        .with_engine(std::sync::Arc::new(ErrorEngine));

        let vq = crate::engine::VendorQuery {
            payload: serde_json::json!({}),
        };
        let solution = AnalyticsSolution {
            payload: SolutionPayload::Vendor(vq),
            solution_source: SolutionSource::VendorEngine("error_vendor".to_string()),
            connector_name: "default".to_string(),
        };

        let err = s.execute_solution(solution).await.unwrap_err().0;
        assert!(
            matches!(&err, AnalyticsError::VendorError { vendor_name, .. } if vendor_name == "error_vendor"),
            "expected VendorError, got: {err:?}"
        );
    }
}
