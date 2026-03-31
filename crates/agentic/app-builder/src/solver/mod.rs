//! App builder domain solver — split across per-state submodules.
//!
//! Each pipeline state has its own module:
//! - [`clarifying`]   — Clarify (triage + ground)
//! - [`specifying`]   — Specify (schema exploration + task/control planning)
//! - [`solving`]      — Solve (SQL generation per task)
//! - [`executing`]    — Execute (connector dispatch)
//! - [`interpreting`] — Interpret (YAML generation)
//! - [`diagnosing`]   — Diagnose (error → recovery routing)
//! - [`prompts`]      — Shared prompt constants and formatting helpers

pub(crate) mod clarifying;
pub(crate) mod diagnosing;
pub(crate) mod executing;
pub(crate) mod interpreting;
pub(crate) mod prompts;
pub(crate) mod resuming;
pub(crate) mod solving;
pub(crate) mod specifying;

mod solver;
pub use solver::AppBuilderSolver;

mod domain_solver;
mod fanout;

use std::collections::HashMap;

use agentic_core::orchestrator::StateHandler;

use crate::events::AppBuilderEvent;
use crate::types::AppBuilderDomain;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Strip markdown JSON fences and whitespace from LLM output.
pub(crate) fn strip_json_fences(raw: &str) -> &str {
    let s = raw.trim();
    let s = s
        .strip_prefix("```json")
        .or_else(|| s.strip_prefix("```"))
        .unwrap_or(s);
    let s = s.strip_suffix("```").unwrap_or(s);
    s.trim()
}

// ---------------------------------------------------------------------------
// Table-driven handlers
// ---------------------------------------------------------------------------

/// Build the app-builder-specific state handler table.
pub fn build_app_builder_handlers()
-> HashMap<&'static str, StateHandler<AppBuilderDomain, AppBuilderSolver, AppBuilderEvent>> {
    let mut map = HashMap::new();
    map.insert("clarifying", clarifying::build_clarifying_handler());
    map.insert("specifying", specifying::build_specifying_handler());
    map.insert("solving", solving::build_solving_handler());
    map.insert("executing", executing::build_executing_handler());
    map.insert("interpreting", interpreting::build_interpreting_handler());
    map
}

#[cfg(test)]
mod tests {
    use agentic_analytics::SemanticCatalog;
    use agentic_connector::{ConnectorError, DatabaseConnector, ExecutionResult, SqlDialect};
    use agentic_core::{
        back_target::BackTarget, human_input::SuspendedRunData, state::ProblemState,
    };
    use agentic_llm::{
        Chunk, ContentBlock, LlmClient, LlmError, LlmProvider, ResponseSchema, StopReason,
        ThinkingConfig, ToolCallChunk, Usage,
    };
    use async_trait::async_trait;

    use crate::types::{
        AppBuilderError, AppIntent, AppSpec, ChartPreference, LayoutNode, ResultShape, TaskPlan,
    };

    use super::AppBuilderSolver;

    // ── StubConnector — always returns an error ──────────────────────────────

    struct StubConnector;

    #[async_trait]
    impl DatabaseConnector for StubConnector {
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

    // ── ScriptedProvider — pre-scripted LLM responses ────────────────────────

    struct ScriptedProvider {
        rounds: std::sync::Mutex<std::collections::VecDeque<Vec<Chunk>>>,
        captured: std::sync::Arc<std::sync::Mutex<Vec<Vec<serde_json::Value>>>>,
    }

    impl ScriptedProvider {
        fn new(rounds: Vec<Vec<Chunk>>) -> Self {
            Self {
                rounds: std::sync::Mutex::new(rounds.into()),
                captured: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
            }
        }

        fn new_with_capture(
            rounds: Vec<Vec<Chunk>>,
            captured: std::sync::Arc<std::sync::Mutex<Vec<Vec<serde_json::Value>>>>,
        ) -> Self {
            Self {
                rounds: std::sync::Mutex::new(rounds.into()),
                captured,
            }
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn stream(
            &self,
            _system: &str,
            messages: &[serde_json::Value],
            _tools: &[agentic_core::tools::ToolDef],
            _thinking: &ThinkingConfig,
            _response_schema: Option<&ResponseSchema>,
            _max_tokens_override: Option<u32>,
        ) -> Result<
            std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Chunk, LlmError>> + Send>>,
            LlmError,
        > {
            self.captured.lock().unwrap().push(messages.to_vec());
            let chunks = self.rounds.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(tokio_stream::iter(
                chunks.into_iter().map(Ok::<_, LlmError>),
            )))
        }

        fn assistant_message(&self, blocks: &[ContentBlock]) -> serde_json::Value {
            let content: Vec<serde_json::Value> = blocks
                .iter()
                .map(|b| match b {
                    ContentBlock::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    ContentBlock::ToolUse {
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
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// A `Chunk` sequence that emits some text then signals MaxTokens exhaustion.
    fn chunks_text_then_max_tokens(text: &str) -> Vec<Chunk> {
        vec![
            Chunk::Text(text.to_string()),
            Chunk::Done(Usage {
                input_tokens: 10,
                output_tokens: 4096,
                stop_reason: StopReason::MaxTokens,
                ..Default::default()
            }),
        ]
    }

    fn chunks_tool_call(id: &str, name: &str) -> Vec<Chunk> {
        vec![
            Chunk::ToolCall(ToolCallChunk {
                id: id.to_string(),
                name: name.to_string(),
                input: serde_json::json!({}),
                provider_data: None,
            }),
            Chunk::Done(Usage::default()),
        ]
    }

    fn make_solver_with_provider(provider: impl LlmProvider + 'static) -> AppBuilderSolver {
        AppBuilderSolver::new(
            LlmClient::with_provider(provider),
            SemanticCatalog::empty(),
            Box::new(StubConnector),
        )
    }

    fn make_intent() -> AppIntent {
        AppIntent {
            raw_request: "build a revenue dashboard".into(),
            app_name: Some("Revenue Dashboard".into()),
            desired_metrics: vec!["revenue".into()],
            mentioned_tables: vec!["orders".into()],
            ..Default::default()
        }
    }

    fn make_spec() -> AppSpec {
        AppSpec {
            intent: make_intent(),
            app_name: "Revenue Dashboard".into(),
            description: "Revenue metrics".into(),
            tasks: vec![TaskPlan {
                name: "kpi_total_revenue".into(),
                description: "Total revenue".into(),
                expected_shape: ResultShape::Scalar,
                expected_columns: vec!["revenue".into()],
                control_deps: vec![],
                is_control_source: false,
            }],
            controls: vec![],
            layout: vec![LayoutNode::Chart {
                task: "kpi_total_revenue".into(),
                preferred: ChartPreference::Auto,
            }],
            connector_name: "default".into(),
        }
    }

    // ── MaxToolRoundsReached suspension tests ────────────────────────────────

    /// `solve_impl` must suspend (not fatal-error) when all tool rounds are
    /// consumed, storing suspension data with `suspension_type = "max_tool_rounds"`.
    #[tokio::test]
    async fn solve_impl_max_tool_rounds_stores_suspension_data() {
        // Default max rounds for solving is 10. Provide 11 rounds of tool calls.
        let rounds: Vec<Vec<Chunk>> = (0..=10)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "execute_preview"))
            .collect();
        let provider = ScriptedProvider::new(rounds);
        let mut solver = make_solver_with_provider(provider);

        let spec = make_spec();
        let result = solver.solve_impl(spec, None).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "solve_impl must suspend on MaxToolRoundsReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "solving");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds"),
        );
        assert!(sd.stage_data["extra_rounds"].as_u64().is_some());
        assert!(sd.stage_data["spec"].is_object());
    }

    /// `clarify_ground_phase` must suspend when all tool rounds are consumed.
    #[tokio::test]
    async fn clarify_ground_max_tool_rounds_stores_suspension_data() {
        // Default max rounds for clarifying is 5. Provide 6 rounds.
        let rounds: Vec<Vec<Chunk>> = (0..=5)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "search_catalog"))
            .collect();
        let provider = ScriptedProvider::new(rounds);
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        let result = solver.clarify_ground_phase(intent).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "clarify_ground_phase must suspend on MaxToolRoundsReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "clarifying");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds"),
        );
        assert!(sd.stage_data["extra_rounds"].as_u64().is_some());
        assert!(sd.stage_data["intent"].is_object());
    }

    /// `specify_impl` must suspend when all tool rounds are consumed.
    #[tokio::test]
    async fn specify_impl_max_tool_rounds_stores_suspension_data() {
        // Default max rounds for specifying is 8. Provide 9 rounds.
        let rounds: Vec<Vec<Chunk>> = (0..=8)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "get_column_values"))
            .collect();
        let provider = ScriptedProvider::new(rounds);
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        let result = solver.specify_impl(intent).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "specify_impl must suspend on MaxToolRoundsReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "specifying");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds"),
        );
        assert!(sd.stage_data["extra_rounds"].as_u64().is_some());
        assert!(sd.stage_data["intent"].is_object());
    }

    // ── MaxTokensReached suspension tests ────────────────────────────────────

    /// `solve_impl` must suspend (not fatal-error) on `MaxTokensReached`,
    /// storing `suspension_type = "max_tokens"` and a doubled `max_tokens_override`.
    #[tokio::test]
    async fn solve_impl_max_tokens_stores_suspension_data() {
        let provider = ScriptedProvider::new(vec![chunks_text_then_max_tokens("partial sql")]);
        let mut solver = make_solver_with_provider(provider);

        let spec = make_spec();
        let result = solver.solve_impl(spec, None).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "solve_impl must suspend on MaxTokensReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "solving");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );
        // DEFAULT_MAX_TOKENS (4096) * 2 = 8192
        assert_eq!(sd.stage_data["max_tokens_override"].as_u64(), Some(8192));
        assert!(sd.stage_data["spec"].is_object());
        assert!(sd.stage_data["conversation_history"].is_array());
    }

    /// `clarify_ground_phase` must suspend on `MaxTokensReached`.
    #[tokio::test]
    async fn clarify_ground_max_tokens_stores_suspension_data() {
        let provider = ScriptedProvider::new(vec![chunks_text_then_max_tokens("partial")]);
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        let result = solver.clarify_ground_phase(intent).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "clarify_ground_phase must suspend on MaxTokensReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "clarifying");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );
        assert_eq!(sd.stage_data["max_tokens_override"].as_u64(), Some(8192));
        assert!(sd.stage_data["intent"].is_object());
        assert!(sd.stage_data["conversation_history"].is_array());
    }

    /// `specify_impl` must suspend on `MaxTokensReached`.
    #[tokio::test]
    async fn specify_impl_max_tokens_stores_suspension_data() {
        let provider = ScriptedProvider::new(vec![chunks_text_then_max_tokens("partial spec")]);
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        let result = solver.specify_impl(intent).await;

        assert!(
            matches!(
                result,
                Err((
                    AppBuilderError::NeedsUserInput { .. },
                    BackTarget::Suspend { .. }
                ))
            ),
            "specify_impl must suspend on MaxTokensReached"
        );

        let sd = solver
            .suspension_data
            .take()
            .expect("suspension_data must be set");
        assert_eq!(sd.from_state, "specifying");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );
        assert_eq!(sd.stage_data["max_tokens_override"].as_u64(), Some(8192));
        assert!(sd.stage_data["intent"].is_object());
        assert!(sd.stage_data["conversation_history"].is_array());
    }

    // ── Resume path tests ───────────────────────────────────────────────────

    fn make_suspended_data(from_state: &str, stage_data: serde_json::Value) -> SuspendedRunData {
        SuspendedRunData {
            from_state: from_state.to_string(),
            original_input: "build a revenue dashboard".to_string(),
            trace_id: String::new(),
            stage_data,
            question: "Continue with more rounds?".to_string(),
            suggestions: vec!["Continue".to_string()],
        }
    }

    #[test]
    fn resume_from_solving_reconstructs_solving_state() {
        let spec = make_spec();
        let spec_value = serde_json::to_value(&spec).unwrap();
        let data = make_suspended_data(
            "solving",
            serde_json::json!({
                "spec": spec_value,
                "suspension_type": "max_tool_rounds",
                "extra_rounds": 10,
            }),
        );

        let state = super::resuming::problem_state_from_resume(&data);
        match state {
            ProblemState::Solving(resumed_spec) => {
                assert_eq!(resumed_spec.app_name, "Revenue Dashboard");
                assert_eq!(resumed_spec.tasks.len(), 1);
                assert_eq!(resumed_spec.tasks[0].name, "kpi_total_revenue");
            }
            _ => panic!("expected Solving state"),
        }
    }

    #[test]
    fn resume_from_specifying_reconstructs_specifying_state() {
        let intent = make_intent();
        let intent_value = serde_json::to_value(&intent).unwrap();
        let data = make_suspended_data(
            "specifying",
            serde_json::json!({
                "intent": intent_value,
                "suspension_type": "max_tool_rounds",
                "extra_rounds": 8,
            }),
        );

        let state = super::resuming::problem_state_from_resume(&data);
        match state {
            ProblemState::Specifying(resumed_intent) => {
                assert_eq!(resumed_intent.raw_request, "build a revenue dashboard");
                assert_eq!(
                    resumed_intent.app_name,
                    Some("Revenue Dashboard".to_string())
                );
            }
            _ => panic!("expected Specifying state"),
        }
    }

    // ── interpret_impl summary emission tests ─────────────────────────────

    fn make_app_result() -> crate::types::AppResult {
        crate::types::AppResult {
            task_results: vec![crate::types::TaskResult {
                name: "kpi_total_revenue".into(),
                sql: "SELECT SUM(amount) AS revenue FROM orders".into(),
                columns: vec!["revenue".into()],
                column_types: vec![Some("DOUBLE".into())],
                row_count: 1,
                is_control_source: false,
                expected_shape: crate::types::ResultShape::Scalar,
                expected_columns: vec![],
                sample: agentic_core::result::QueryResult {
                    columns: vec!["revenue".into()],
                    rows: vec![agentic_core::result::QueryRow(vec![
                        agentic_core::result::CellValue::Number(42000.0),
                    ])],
                    total_row_count: 1,
                    truncated: false,
                },
            }],
            controls: vec![],
            layout: vec![crate::types::LayoutNode::Chart {
                task: "kpi_total_revenue".into(),
                preferred: crate::types::ChartPreference::Auto,
            }],
            connector_name: "local".into(),
        }
    }

    /// `interpret_impl` must emit the LLM-generated summary as a `LlmToken`
    /// event so the frontend sees it as `text_delta` in the interpreting step.
    #[tokio::test]
    async fn interpret_impl_emits_summary_as_llm_token() {
        use agentic_core::events::{CoreEvent, Event};

        let summary_text = "A revenue dashboard with 1 chart.";
        // First call: chart config resolution, second call: summary.
        let chart_config_response = r#"[{"task":"kpi_total_revenue","chart_type":"table","x":null,"y":null,"series":null,"name":null,"value":null}]"#;
        let provider = ScriptedProvider::new(vec![
            vec![
                Chunk::Text(chart_config_response.to_string()),
                Chunk::Done(Usage::default()),
            ],
            vec![
                Chunk::Text(summary_text.to_string()),
                Chunk::Done(Usage::default()),
            ],
        ]);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Event<crate::events::AppBuilderEvent>>(64);
        let mut solver = make_solver_with_provider(provider);
        solver.event_tx = Some(tx);

        let result = make_app_result();
        let answer = match solver.interpret_impl(result).await {
            Ok(a) => a,
            Err((e, _)) => panic!("interpret_impl should succeed, got: {e:?}"),
        };

        assert_eq!(answer.summary, summary_text);

        // Drain the event channel and check for a LlmToken with the summary text.
        let mut found_token = false;
        let mut found_yaml_ready = false;
        rx.close();
        while let Some(event) = rx.recv().await {
            match event {
                Event::Core(CoreEvent::LlmToken { token, .. }) if token == summary_text => {
                    found_token = true;
                }
                Event::Domain(crate::events::AppBuilderEvent::AppYamlReady { .. }) => {
                    found_yaml_ready = true;
                }
                _ => {}
            }
        }

        assert!(
            found_token,
            "interpret_impl must emit summary as LlmToken event"
        );
        assert!(
            found_yaml_ready,
            "interpret_impl must emit AppYamlReady event"
        );
    }

    /// `specify_impl` on resume uses `InitialMessages::Messages` (not a fresh prompt).
    #[tokio::test]
    async fn specifying_resume_uses_continue_messages() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        // 9 tool-call rounds for initial suspension (max=8), then a valid spec response.
        // With max_tool_rounds=8: rounds 0..7 are processed (8 stream calls), then
        // the 9th call (index 8) triggers MaxToolRoundsReached (rounds=8 >= max=8).
        // The resume run gets the spec_json terminal response at captured index 9.
        let spec_json = serde_json::json!({
            "app_name": "Revenue Dashboard",
            "description": "desc",
            "tasks": [{
                "name": "kpi_revenue",
                "description": "total revenue",
                "expected_shape": "scalar",
                "expected_columns": ["revenue"],
                "control_deps": [],
                "is_control_source": false,
            }],
            "controls": [],
            "layout": [{"type": "chart", "task": "kpi_revenue", "preferred": "auto"}],
        });
        let mut rounds: Vec<Vec<Chunk>> = (0..=8)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "get_column_values"))
            .collect();
        rounds.push(vec![
            Chunk::ToolCall(ToolCallChunk {
                id: "resp".into(),
                name: "specify_response".into(),
                input: spec_json,
                provider_data: None,
            }),
            Chunk::Done(Usage::default()),
        ]);
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        // First run: should suspend.
        let intent = make_intent();
        let _ = solver.specify_impl(intent.clone()).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds")
        );

        // Set resume_data.
        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue".to_string(),
        });

        // Second run: should use build_continue_messages.
        let _ = solver.specify_impl(intent).await;

        let calls = captured.lock().unwrap();
        // First run uses 9 stream calls (captured[0..8]); the resume's first call is at index 9.
        let resume_call_msgs = calls.get(9).expect("should have a resume LLM call");
        let last_msg = resume_call_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_call_msgs.len() > 1, "must include stored history");
    }

    /// `clarify_ground_phase` on resume uses `InitialMessages::Messages` (not a fresh prompt).
    #[tokio::test]
    async fn clarify_ground_resume_uses_continue_messages() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        // 6 tool-call rounds for initial suspension (max=5), then a valid triage response.
        // With max_tool_rounds=5: rounds 0..4 are processed (5 stream calls), then
        // the 6th call (index 5) triggers MaxToolRoundsReached (rounds=5 >= max=5).
        // The resume run gets the triage_json terminal response at captured index 6.
        let triage_json = serde_json::json!({
            "app_name": "Revenue Dashboard",
            "description": "desc",
            "desired_metrics": ["revenue"],
            "desired_controls": [],
            "mentioned_tables": ["orders"],
            "ambiguities": [],
        });
        let mut rounds: Vec<Vec<Chunk>> = (0..=5)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "search_catalog"))
            .collect();
        rounds.push(vec![
            Chunk::ToolCall(ToolCallChunk {
                id: "resp".into(),
                name: "triage_response".into(),
                input: triage_json,
                provider_data: None,
            }),
            Chunk::Done(Usage::default()),
        ]);
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        // First run: suspend.
        let _ = solver.clarify_ground_phase(intent.clone()).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds")
        );

        // Set resume_data.
        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue".to_string(),
        });

        // Second run: should use build_continue_messages.
        let _ = solver.clarify_ground_phase(intent).await;

        let calls = captured.lock().unwrap();
        // First run uses 6 stream calls (captured[0..5]); the resume's first call is at index 6.
        let resume_call_msgs = calls.get(6).expect("should have a resume LLM call");
        let last_msg = resume_call_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_call_msgs.len() > 1, "must include stored history");
    }

    /// `solve_impl` on resume uses `InitialMessages::Messages` (not a fresh prompt).
    #[tokio::test]
    async fn solve_impl_resume_uses_continue_messages() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        // 11 tool-call rounds for initial suspension (max=10), then a valid solve response.
        // With max_tool_rounds=10: rounds 0..9 are processed (10 stream calls), then
        // the 11th call (index 10) triggers MaxToolRoundsReached (rounds=10 >= max=10).
        // The resume run gets the solve_json terminal response at captured index 11.
        let solve_json = serde_json::json!({ "sql": "SELECT SUM(amount) FROM orders" });
        let mut rounds: Vec<Vec<Chunk>> = (0..=10)
            .map(|i| chunks_tool_call(&format!("tc{i}"), "execute_preview"))
            .collect();
        rounds.push(vec![
            Chunk::ToolCall(ToolCallChunk {
                id: "resp".into(),
                name: "solve_response".into(),
                input: solve_json,
                provider_data: None,
            }),
            Chunk::Done(Usage::default()),
        ]);
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        let spec = make_spec();
        // First run: suspend.
        let _ = solver.solve_impl(spec.clone(), None).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tool_rounds")
        );

        // Set resume_data.
        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue".to_string(),
        });

        // Second run: should use build_continue_messages.
        let _ = solver.solve_impl(spec, None).await;

        let calls = captured.lock().unwrap();
        // First run uses 11 stream calls (captured[0..10]); the resume's first call is at index 11.
        let resume_call_msgs = calls.get(11).expect("should have a resume LLM call");
        let last_msg = resume_call_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_call_msgs.len() > 1, "must include stored history");
    }

    /// On resume from a `max_tokens` suspension in `solve_impl`, the first LLM
    /// call must use `build_continue_messages` (not a fresh prompt) and the
    /// `max_tokens_override` must be forwarded.
    #[tokio::test]
    async fn solve_impl_max_tokens_resume_uses_continue_messages_and_override() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        // First round hits MaxTokens; second round returns a valid solve response.
        let solve_json = serde_json::json!({ "sql": "SELECT SUM(amount) FROM orders" });
        let rounds = vec![
            chunks_text_then_max_tokens("partial"),
            vec![
                Chunk::ToolCall(ToolCallChunk {
                    id: "resp".into(),
                    name: "solve_response".into(),
                    input: solve_json,
                    provider_data: None,
                }),
                Chunk::Done(Usage::default()),
            ],
        ];
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        let spec = make_spec();
        // First run: should suspend on MaxTokensReached.
        let _ = solver.solve_impl(spec.clone(), None).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );

        // Set resume_data.
        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue with double budget".to_string(),
        });

        // Second run: should use build_continue_messages.
        let _ = solver.solve_impl(spec, None).await;

        let calls = captured.lock().unwrap();
        let resume_msgs = calls.get(1).expect("should have a resume LLM call");
        let last_msg = resume_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_msgs.len() > 1, "must include stored history");
    }

    /// On resume from a `max_tokens` suspension in `specify_impl`, the first
    /// LLM call must use `build_continue_messages`.
    #[tokio::test]
    async fn specify_impl_max_tokens_resume_uses_continue_messages() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let spec_json = serde_json::json!({
            "app_name": "Revenue Dashboard",
            "description": "desc",
            "tasks": [{
                "name": "kpi_revenue",
                "description": "total revenue",
                "expected_shape": "scalar",
                "expected_columns": ["revenue"],
                "control_deps": [],
                "is_control_source": false,
            }],
            "controls": [],
            "layout": [{"type": "chart", "task": "kpi_revenue", "preferred": "auto"}],
        });
        let rounds = vec![
            chunks_text_then_max_tokens("partial"),
            vec![
                Chunk::ToolCall(ToolCallChunk {
                    id: "resp".into(),
                    name: "specify_response".into(),
                    input: spec_json,
                    provider_data: None,
                }),
                Chunk::Done(Usage::default()),
            ],
        ];
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        // First run: should suspend.
        let _ = solver.specify_impl(intent.clone()).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );

        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue with double budget".to_string(),
        });

        let _ = solver.specify_impl(intent).await;

        let calls = captured.lock().unwrap();
        let resume_msgs = calls.get(1).expect("should have a resume LLM call");
        let last_msg = resume_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_msgs.len() > 1, "must include stored history");
    }

    /// On resume from a `max_tokens` suspension in `clarify_ground_phase`, the
    /// first LLM call must use `build_continue_messages`.
    #[tokio::test]
    async fn clarify_ground_max_tokens_resume_uses_continue_messages() {
        use agentic_core::human_input::ResumeInput;

        let captured = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let triage_json = serde_json::json!({
            "app_name": "Revenue Dashboard",
            "description": "desc",
            "desired_metrics": ["revenue"],
            "desired_controls": [],
            "mentioned_tables": ["orders"],
            "ambiguities": [],
        });
        let rounds = vec![
            chunks_text_then_max_tokens("partial"),
            vec![
                Chunk::ToolCall(ToolCallChunk {
                    id: "resp".into(),
                    name: "triage_response".into(),
                    input: triage_json,
                    provider_data: None,
                }),
                Chunk::Done(Usage::default()),
            ],
        ];
        let provider = ScriptedProvider::new_with_capture(rounds, std::sync::Arc::clone(&captured));
        let mut solver = make_solver_with_provider(provider);

        let intent = make_intent();
        // First run: suspend on MaxTokensReached.
        let _ = solver.clarify_ground_phase(intent.clone()).await;
        let sd = solver.suspension_data.take().expect("must have suspension");
        assert_eq!(
            sd.stage_data["suspension_type"].as_str(),
            Some("max_tokens")
        );

        solver.resume_data = Some(ResumeInput {
            data: sd,
            answer: "Continue with double budget".to_string(),
        });

        let _ = solver.clarify_ground_phase(intent).await;

        let calls = captured.lock().unwrap();
        let resume_msgs = calls.get(1).expect("should have a resume LLM call");
        let last_msg = resume_msgs.last().expect("messages non-empty");
        assert_eq!(last_msg["role"].as_str(), Some("user"));
        assert_eq!(last_msg["content"].as_str(), Some("Please continue."));
        assert!(resume_msgs.len() > 1, "must include stored history");
    }

    #[test]
    fn resume_from_solving_with_bad_spec_falls_back_to_clarifying() {
        let data = make_suspended_data(
            "solving",
            serde_json::json!({
                "spec": "not a valid spec",
                "suspension_type": "max_tool_rounds",
            }),
        );

        let state = super::resuming::problem_state_from_resume(&data);
        assert!(
            matches!(state, ProblemState::Clarifying(_)),
            "bad spec should fall back to Clarifying"
        );
    }
}
