//! Workflow execution observability events
//!
//! This module provides utilities for logging workflow and task execution events
//! with consistent OpenTelemetry span creation and field recording.

use tracing::{Level, Span, event};

/// Constants and logging functions for workflow service entry points (run_workflow, run_workflow_v2)
pub mod run_workflow {
    use super::*;

    pub static NAME: &str = "workflow.run_workflow";
    pub static TYPE: &str = "workflow";
    pub static INPUT: &str = "workflow.run_workflow.input";
    pub static OUTPUT: &str = "workflow.run_workflow.output";

    pub fn input(workflow_ref: &str, retry_strategy: &str) {
        event!(
            Level::INFO,
            name = INPUT,
            is_visible = true,
            workflow_ref = %workflow_ref,
            retry_strategy = %retry_strategy
        );
    }

    pub fn output(output: &crate::execute::types::OutputContainer) {
        event!(
            Level::INFO,
            name = OUTPUT,
            is_visible = true,
            status = "success",
            output = %serde_json::to_string(output).unwrap_or_default()
        );
    }

    pub fn error(error: &str) {
        event!(
            Level::ERROR,
            name = OUTPUT,
            is_visible = true,
            status = "error",
            error = %error
        );
    }
}

/// Constants and logging functions for workflow launcher spans
pub mod launcher {
    use super::*;

    pub mod with_project {
        use super::*;

        pub static NAME: &str = "workflow.launcher.with_project";
        pub static TYPE: &str = "workflow";
        pub static INPUT: &str = "workflow.launcher.with_project.input";
        pub static OUTPUT: &str = "workflow.launcher.with_project.output";

        pub fn input(project_path: &str) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                project_path = %project_path
            );
        }

        pub fn output() {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success"
            );
        }
    }

    pub mod get_global_context {
        use super::*;

        pub static NAME: &str = "workflow.launcher.get_global_context";
        pub static TYPE: &str = "workflow";
        pub static INPUT: &str = "workflow.launcher.get_global_context.input";
        pub static OUTPUT: &str = "workflow.launcher.get_global_context.output";

        pub fn input() {
            event!(Level::INFO, name = INPUT, is_visible = true);
        }

        pub fn output(has_models: bool, has_dimensions: bool, has_globals: bool) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                has_models = has_models,
                has_dimensions = has_dimensions,
                has_globals = has_globals
            );
        }
    }

    pub mod launch {
        use super::*;

        pub static NAME: &str = "workflow.launcher.launch";
        pub static TYPE: &str = "workflow";
        pub static INPUT: &str = "workflow.launcher.launch.input";
        pub static OUTPUT: &str = "workflow.launcher.launch.output";

        pub fn input(workflow_ref: &str, retry_strategy: &str) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                workflow_ref = %workflow_ref,
                retry_strategy = %retry_strategy
            );
        }

        pub fn output(output: &crate::execute::types::OutputContainer) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }

        pub fn error(error: &str) {
            event!(
                Level::ERROR,
                name = OUTPUT,
                is_visible = true,
                status = "error",
                error = %error
            );
        }
    }
}

/// Constants and logging functions for task execution spans
pub mod task {
    use super::*;

    pub mod execute {
        use super::*;

        pub static NAME: &str = "workflow.task.execute";
        pub static TYPE: &str = "task";
        pub static INPUT: &str = "workflow.task.execute.input";
        pub static OUTPUT: &str = "workflow.task.execute.output";

        pub fn input(task_name: &str, task_type: &str) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                task_name = %task_name,
                task_type = %task_type
            );
        }

        pub fn output(task_name: &str, output: &crate::execute::types::OutputContainer) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                task_name = %task_name,
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }

        pub fn error(task_name: &str, error: &str) {
            event!(
                Level::ERROR,
                name = OUTPUT,
                is_visible = true,
                status = "error",
                task_name = %task_name,
                error = %error
            );
        }
    }

    pub mod agent {
        use super::*;

        pub static NAME: &str = "workflow.task.agent.execute";
        pub static TYPE: &str = "agent";
        pub static INPUT: &str = "workflow.task.agent.input";
        pub static OUTPUT: &str = "workflow.task.agent.output";

        pub fn input(agent_ref: &str, prompt: &str, consistency_run: usize) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                agent_ref = %agent_ref,
                prompt = %prompt,
                consistency_run = consistency_run
            );
        }

        pub fn output(agent_ref: &str, output: &crate::execute::types::OutputContainer) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                agent_ref = %agent_ref,
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }
    }

    pub mod semantic_query {
        use super::*;

        pub static NAME_RENDER: &str = "workflow.task.semantic_query.render";
        pub static NAME_MAP: &str = "workflow.task.semantic_query.map";
        pub static NAME_COMPILE: &str = "workflow.task.semantic_query.compile";
        pub static NAME_EXECUTE: &str = "workflow.task.semantic_query.execute";
        pub static NAME_GET_SQL_FROM_CUBEJS: &str =
            "workflow.task.semantic_query.get_sql_from_cubejs";
        pub static NAME_EXECUTE_SQL: &str = "workflow.task.semantic_query.execute_sql";
        pub static TYPE: &str = "semantic_query";

        pub static INPUT_RENDER: &str = "workflow.task.semantic_query.render.input";
        pub static OUTPUT_RENDER: &str = "workflow.task.semantic_query.render.output";
        pub static INPUT_MAP: &str = "workflow.task.semantic_query.map.input";
        pub static OUTPUT_MAP: &str = "workflow.task.semantic_query.map.output";
        pub static INPUT_COMPILE: &str = "workflow.task.semantic_query.compile.input";
        pub static OUTPUT_COMPILE: &str = "workflow.task.semantic_query.compile.output";
        pub static INPUT_EXECUTE: &str = "workflow.task.semantic_query.execute.input";
        pub static OUTPUT_EXECUTE: &str = "workflow.task.semantic_query.execute.output";
        pub static INPUT_GET_SQL: &str = "workflow.task.semantic_query.get_sql_from_cubejs.input";
        pub static OUTPUT_GET_SQL: &str = "workflow.task.semantic_query.get_sql_from_cubejs.output";
        pub static INPUT_EXECUTE_SQL: &str = "workflow.task.semantic_query.execute_sql.input";
        pub static OUTPUT_EXECUTE_SQL: &str = "workflow.task.semantic_query.execute_sql.output";

        pub fn render_input(task: &crate::config::model::SemanticQueryTask) {
            event!(
                Level::INFO,
                name = INPUT_RENDER,
                is_visible = true,
                task = %serde_json::to_string(task).unwrap_or_default()
            );
        }

        pub fn render_output(rendered_task: &crate::config::model::SemanticQueryTask) {
            event!(
                Level::INFO,
                name = OUTPUT_RENDER,
                is_visible = true,
                status = "success",
                rendered_task = %serde_json::to_string(rendered_task).unwrap_or_default()
            );
        }

        pub fn map_input(task: &crate::config::model::SemanticQueryTask) {
            event!(
                Level::INFO,
                name = INPUT_MAP,
                is_visible = true,
                task = %serde_json::to_string(task).unwrap_or_default()
            );
        }

        pub fn map_output(topic: &str, dimensions_count: usize, measures_count: usize) {
            event!(
                Level::INFO,
                name = OUTPUT_MAP,
                is_visible = true,
                status = "success",
                topic = %topic,
                dimensions_count = dimensions_count,
                measures_count = measures_count
            );
        }

        pub fn compile_input(topic: &str, query: &crate::service::types::SemanticQueryParams) {
            event!(
                Level::INFO,
                name = INPUT_COMPILE,
                is_visible = true,
                topic = %topic,
                query = %serde_json::to_string(query).unwrap_or_default()
            );
        }

        pub fn compile_output(sql: &str) {
            event!(
                Level::INFO,
                name = OUTPUT_COMPILE,
                is_visible = true,
                status = "success",
                sql = %sql
            );
        }

        pub fn execute_input(topic: &str, dimensions: &[String], measures: &[String]) {
            event!(
                Level::INFO,
                name = INPUT_EXECUTE,
                is_visible = true,
                topic = %topic,
                dimensions = %serde_json::to_string(dimensions).unwrap_or_default(),
                measures = %serde_json::to_string(measures).unwrap_or_default()
            );
        }

        pub fn execute_output(output: &crate::execute::types::Output) {
            event!(
                Level::INFO,
                name = OUTPUT_EXECUTE,
                is_visible = true,
                status = "success",
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }

        pub fn get_sql_input(cubejs_query: &serde_json::Value) {
            event!(
                Level::INFO,
                name = INPUT_GET_SQL,
                is_visible = true,
                cubejs_query = %cubejs_query
            );
        }

        pub fn get_sql_output(sql: &str) {
            event!(
                Level::INFO,
                name = OUTPUT_GET_SQL,
                is_visible = true,
                status = "success",
                sql = %sql
            );
        }

        pub fn execute_sql_input(database_ref: &str, sql: &str) {
            event!(
                Level::INFO,
                name = INPUT_EXECUTE_SQL,
                is_visible = true,
                database_ref = %database_ref,
                sql = %sql
            );
        }

        pub fn execute_sql_output(file_path: &str) {
            event!(
                Level::INFO,
                name = OUTPUT_EXECUTE_SQL,
                is_visible = true,
                status = "success",
                file_path = %file_path
            );
        }
    }

    pub mod execute_sql {
        use super::*;

        pub static NAME_MAP: &str = "workflow.task.execute_sql.map";
        pub static NAME_EXECUTE: &str = "workflow.task.execute_sql.execute";
        pub static TYPE: &str = "execute_sql";

        pub static INPUT_MAP: &str = "workflow.task.execute_sql.map.input";
        pub static OUTPUT_MAP: &str = "workflow.task.execute_sql.map.output";
        pub static INPUT_EXECUTE: &str = "workflow.task.execute_sql.execute.input";
        pub static OUTPUT_EXECUTE: &str = "workflow.task.execute_sql.execute.output";

        pub fn map_input(task: &crate::config::model::ExecuteSQLTask) {
            event!(
                Level::INFO,
                name = INPUT_MAP,
                is_visible = true,
                database = %task.database,
                sql = %serde_json::to_string(&task.sql).unwrap_or_default()
            );
        }

        pub fn map_output(sql: &str, database: &str) {
            event!(
                Level::INFO,
                name = OUTPUT_MAP,
                is_visible = true,
                status = "success",
                sql = %sql,
                database = %database
            );
        }

        pub fn execute_input(database: &str, sql: &str, dry_run_limit: Option<usize>) {
            event!(
                Level::INFO,
                name = INPUT_EXECUTE,
                is_visible = true,
                database = %database,
                sql = %sql,
                dry_run_limit = ?dry_run_limit
            );
        }

        pub fn execute_output(file_path: &str, row_count: Option<usize>) {
            event!(
                Level::INFO,
                name = OUTPUT_EXECUTE,
                is_visible = true,
                status = "success",
                file_path = %file_path,
                row_count = ?row_count
            );
        }
    }

    pub mod omni_query {
        use super::*;

        pub static NAME_MAP: &str = "workflow.task.omni_query.map";
        pub static NAME_EXECUTE: &str = "workflow.task.omni_query.execute";
        pub static NAME_EXECUTE_QUERY: &str = "workflow.task.omni_query.execute_query";
        pub static TYPE: &str = "omni_query";

        pub static INPUT_MAP: &str = "workflow.task.omni_query.map.input";
        pub static OUTPUT_MAP: &str = "workflow.task.omni_query.map.output";
        pub static INPUT_EXECUTE: &str = "workflow.task.omni_query.execute.input";
        pub static OUTPUT_EXECUTE: &str = "workflow.task.omni_query.execute.output";

        pub fn map_input(task: &crate::config::model::OmniQueryTask) {
            event!(
                Level::INFO,
                name = INPUT_MAP,
                is_visible = true,
                integration = %task.integration,
                topic = %task.topic,
                fields = %serde_json::to_string(&task.query.fields).unwrap_or_default()
            );
        }

        pub fn map_output(integration: &str, topic: &str, fields_count: usize) {
            event!(
                Level::INFO,
                name = OUTPUT_MAP,
                is_visible = true,
                status = "success",
                integration = %integration,
                topic = %topic,
                fields_count = fields_count
            );
        }

        pub fn execute_input(
            integration: &str,
            topic: &str,
            params: &crate::types::tool_params::OmniQueryParams,
        ) {
            event!(
                Level::INFO,
                name = INPUT_EXECUTE,
                is_visible = true,
                integration = %integration,
                topic = %topic,
                params = %serde_json::to_string(params).unwrap_or_default()
            );
        }

        pub fn execute_output(output: &crate::execute::types::Output) {
            event!(
                Level::INFO,
                name = OUTPUT_EXECUTE,
                is_visible = true,
                status = "success",
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }
    }

    pub mod loop_task {
        use super::*;

        pub static NAME_MAP: &str = "workflow.task.loop.map";
        pub static NAME_ITEM_MAP: &str = "workflow.task.loop.item_map";
        pub static TYPE: &str = "loop";

        pub static INPUT_MAP: &str = "workflow.task.loop.map.input";
        pub static OUTPUT_MAP: &str = "workflow.task.loop.map.output";
        pub static INPUT_ITEM_MAP: &str = "workflow.task.loop.item_map.input";
        pub static OUTPUT_ITEM_MAP: &str = "workflow.task.loop.item_map.output";

        pub fn map_input(values_count: usize) {
            event!(
                Level::INFO,
                name = INPUT_MAP,
                is_visible = true,
                values_count = values_count
            );
        }

        pub fn map_output(iterations_count: usize) {
            event!(
                Level::INFO,
                name = OUTPUT_MAP,
                is_visible = true,
                status = "success",
                iterations_count = iterations_count
            );
        }

        pub fn item_map_input(iteration_index: usize, task_name: &str, value: &str) {
            event!(
                Level::INFO,
                name = INPUT_ITEM_MAP,
                is_visible = true,
                iteration_index = iteration_index,
                task_name = %task_name,
                value = %value
            );
        }

        pub fn item_map_output(iteration_index: usize, task_name: &str) {
            event!(
                Level::INFO,
                name = OUTPUT_ITEM_MAP,
                is_visible = true,
                status = "success",
                iteration_index = iteration_index,
                task_name = %task_name
            );
        }
    }

    pub mod formatter {
        use super::*;

        pub static NAME_EXECUTE: &str = "workflow.task.formatter.execute";
        pub static TYPE: &str = "formatter";
        pub static INPUT: &str = "workflow.task.formatter.input";
        pub static OUTPUT: &str = "workflow.task.formatter.output";

        pub fn input(template: &str) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                template = %template
            );
        }

        pub fn output(result: &str) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                result = %result
            );
        }
    }

    pub mod sub_workflow {
        use super::*;

        pub static NAME_EXECUTE: &str = "workflow.task.sub_workflow.execute";
        pub static TYPE: &str = "sub_workflow";
        pub static INPUT: &str = "workflow.task.sub_workflow.input";
        pub static OUTPUT: &str = "workflow.task.sub_workflow.output";

        pub fn input(workflow_ref: &str) {
            event!(
                Level::INFO,
                name = INPUT,
                is_visible = true,
                workflow_ref = %workflow_ref
            );
        }

        pub fn output(workflow_ref: &str, output: &crate::execute::types::OutputContainer) {
            event!(
                Level::INFO,
                name = OUTPUT,
                is_visible = true,
                status = "success",
                workflow_ref = %workflow_ref,
                output = %serde_json::to_string(output).unwrap_or_default()
            );
        }
    }
}

/// Creates a span for workflow launcher initialization
pub fn workflow_launcher_with_project_span() -> Span {
    tracing::info_span!(
        "workflow.launcher.with_project",
        otel.name = launcher::with_project::NAME,
        oxy.span_type = launcher::with_project::TYPE,
    )
}

/// Creates a span for workflow global context retrieval
pub fn workflow_launcher_get_global_context_span() -> Span {
    tracing::info_span!(
        "workflow.launcher.get_global_context",
        otel.name = launcher::get_global_context::NAME,
        oxy.span_type = launcher::get_global_context::TYPE,
    )
}

/// Creates a span for workflow launch
pub fn workflow_launcher_launch_span(workflow_ref: &str) -> Span {
    tracing::info_span!(
        "workflow.launcher.launch",
        otel.name = launcher::launch::NAME,
        oxy.span_type = launcher::launch::TYPE,
        oxy.workflow.ref = %workflow_ref,
    )
}

/// Creates a span for task execution
pub fn task_execute_span(task_name: &str) -> Span {
    tracing::info_span!(
        "workflow.task.execute",
        otel.name = task::execute::NAME,
        oxy.span_type = task::execute::TYPE,
        oxy.task.name = %task_name,
    )
}

/// Creates a span for agent task execution within a workflow
pub fn task_agent_execute_span(agent_ref: &str, consistency_run: usize) -> Span {
    tracing::info_span!(
        "workflow.task.agent.execute",
        otel.name = task::agent::NAME,
        oxy.span_type = task::agent::TYPE,
        oxy.agent.ref = %agent_ref,
        oxy.agent.consistency_run = consistency_run,
    )
}

/// Creates a span for semantic query task rendering
pub fn task_semantic_query_render_span(
    topic: Option<&str>,
    dimensions_count: usize,
    measures_count: usize,
    filters_count: usize,
) -> Span {
    let span = tracing::info_span!(
        "workflow.task.semantic_query.render",
        otel.name = task::semantic_query::NAME_RENDER,
        oxy.span_type = task::semantic_query::TYPE,
        oxy.semantic_query.topic = tracing::field::Empty,
        oxy.semantic_query.dimensions_count = dimensions_count,
        oxy.semantic_query.measures_count = measures_count,
        oxy.semantic_query.filters_count = filters_count,
    );
    if let Some(topic) = topic {
        span.record("oxy.semantic_query.topic", topic);
    }
    span
}

/// Creates a span for semantic query task mapping
pub fn task_semantic_query_map_span() -> Span {
    tracing::info_span!(
        "workflow.task.semantic_query.map",
        otel.name = task::semantic_query::NAME_MAP,
        oxy.span_type = task::semantic_query::TYPE,
    )
}

/// Creates a span for semantic query compilation
pub fn task_semantic_query_compile_span(topic: &str) -> Span {
    tracing::info_span!(
        "workflow.task.semantic_query.compile",
        otel.name = task::semantic_query::NAME_COMPILE,
        oxy.span_type = task::semantic_query::TYPE,
        oxy.semantic_query.topic = %topic,
    )
}

/// Creates a span for semantic query execution
pub fn task_semantic_query_execute_span(
    topic: &str,
    dimensions_count: usize,
    measures_count: usize,
    filters_count: usize,
) -> Span {
    tracing::info_span!(
        "workflow.task.semantic_query.execute",
        otel.name = task::semantic_query::NAME_EXECUTE,
        oxy.span_type = task::semantic_query::TYPE,
        oxy.semantic_query.topic = %topic,
        oxy.semantic_query.dimensions_count = dimensions_count,
        oxy.semantic_query.measures_count = measures_count,
        oxy.semantic_query.filters_count = filters_count,
    )
}

/// Creates a span for CubeJS SQL generation
pub fn task_semantic_query_get_sql_from_cubejs_span() -> Span {
    tracing::info_span!(
        "workflow.task.semantic_query.get_sql_from_cubejs",
        otel.name = task::semantic_query::NAME_GET_SQL_FROM_CUBEJS,
        oxy.span_type = task::semantic_query::TYPE,
    )
}

/// Creates a span for semantic query SQL execution
pub fn task_semantic_query_execute_sql_span(database_ref: &str) -> Span {
    tracing::info_span!(
        "workflow.task.semantic_query.execute_sql",
        otel.name = task::semantic_query::NAME_EXECUTE_SQL,
        oxy.span_type = task::semantic_query::TYPE,
        oxy.database.ref = database_ref,
    )
}

/// Creates a span for SQL task mapping
pub fn task_execute_sql_map_span(database_ref: &str) -> Span {
    tracing::info_span!(
        "workflow.task.execute_sql.map",
        otel.name = task::execute_sql::NAME_MAP,
        oxy.span_type = task::execute_sql::TYPE,
        oxy.database.ref = %database_ref,
    )
}

/// Creates a span for SQL task execution
pub fn task_execute_sql_execute_span(database_ref: &str, dry_run_limit: Option<usize>) -> Span {
    let span = tracing::info_span!(
        "workflow.task.execute_sql.execute",
        otel.name = task::execute_sql::NAME_EXECUTE,
        oxy.span_type = task::execute_sql::TYPE,
        oxy.database.ref = %database_ref,
        oxy.sql.dry_run_limit = tracing::field::Empty,
    );
    if let Some(limit) = dry_run_limit {
        span.record("oxy.sql.dry_run_limit", limit);
    }
    span
}

/// Creates a span for OmniQuery task mapping
pub fn task_omni_query_map_span(integration: &str, topic: &str) -> Span {
    tracing::info_span!(
        "workflow.task.omni_query.map",
        otel.name = task::omni_query::NAME_MAP,
        oxy.span_type = task::omni_query::TYPE,
        oxy.omni_query.integration = %integration,
        oxy.omni_query.topic = %topic,
    )
}

/// Creates a span for OmniQuery task execution
pub fn task_omni_query_execute_span(integration: &str, topic: &str, fields_count: usize) -> Span {
    tracing::info_span!(
        "workflow.task.omni_query.execute",
        otel.name = task::omni_query::NAME_EXECUTE,
        oxy.span_type = task::omni_query::TYPE,
        oxy.omni_query.integration = %integration,
        oxy.omni_query.topic = %topic,
        oxy.omni_query.fields_count = fields_count,
    )
}

/// Creates a span for OmniQuery execution logic
pub fn task_omni_query_execute_query_span(
    integration: &str,
    topic: &str,
    fields_count: usize,
) -> Span {
    tracing::info_span!(
        "workflow.task.omni_query.execute_query",
        otel.name = task::omni_query::NAME_EXECUTE_QUERY,
        oxy.span_type = task::omni_query::TYPE,
        oxy.omni_query.integration = integration,
        oxy.omni_query.topic = topic,
        oxy.omni_query.fields_count = fields_count,
    )
}

/// Creates a span for loop task mapping
pub fn task_loop_map_span(iterations_count: usize) -> Span {
    tracing::info_span!(
        "workflow.task.loop.map",
        otel.name = task::loop_task::NAME_MAP,
        oxy.span_type = task::loop_task::TYPE,
        oxy.loop.iterations_count = iterations_count,
    )
}

/// Creates a span for loop item mapping
pub fn task_loop_item_map_span(iteration_index: usize, task_name: &str) -> Span {
    tracing::info_span!(
        "workflow.task.loop.item_map",
        otel.name = task::loop_task::NAME_ITEM_MAP,
        oxy.span_type = task::loop_task::TYPE,
        oxy.loop.iteration_index = iteration_index,
        oxy.loop.task_name = task_name,
    )
}

/// Creates a span for formatter task execution
pub fn task_formatter_execute_span() -> Span {
    tracing::info_span!(
        "workflow.task.formatter.execute",
        otel.name = task::formatter::NAME_EXECUTE,
        oxy.span_type = task::formatter::TYPE,
    )
}

/// Creates a span for sub-workflow execution
pub fn task_sub_workflow_execute_span(workflow_ref: &str) -> Span {
    tracing::info_span!(
        "workflow.task.sub_workflow.execute",
        otel.name = task::sub_workflow::NAME_EXECUTE,
        oxy.span_type = task::sub_workflow::TYPE,
        oxy.workflow.ref = %workflow_ref,
    )
}

/// Helper to record execution source in the current span
/// Generic version to avoid circular dependencies
pub fn record_execution_source(span: &Span, source: &impl serde::Serialize) {
    // Record as JSON string since we can't import the ExecutionSource type
    span.record(
        "oxy.execution.source",
        serde_json::to_string(source).unwrap_or_default().as_str(),
    );
}

/// Logs a workflow event at info level
pub fn log_workflow_event(event_name: &str, message: &str) {
    tracing::event!(
        Level::INFO,
        name = event_name,
        message = %message,
    );
}

/// Logs a task event at info level
pub fn log_task_event(task_name: &str, event_name: &str, message: &str) {
    tracing::event!(
        Level::INFO,
        name = event_name,
        task_name = %task_name,
        message = %message,
    );
}
