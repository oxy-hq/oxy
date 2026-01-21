//! Metric collection context for accumulating usage data during execution
//!
//! This module provides a context accumulator that collects metric usage data
//! as it flows through the execution pipeline (agent, workflow, task).
//!
//! ## Usage Pattern
//!
//! The `MetricContext` is passed through `ExecutionContext` and works correctly
//! with nested agent/workflow executions and `tokio::spawn`:
//!
//! ```rust,ignore
//! // Context is created in launcher and attached to ExecutionContext
//! let metric_ctx = MetricContext::new(SourceType::Agent, "my-agent");
//! let execution_context = execution_context.with_metric_context(metric_ctx);
//!
//! // In spawned tasks, access via execution_context
//! execution_context.record_sql("SELECT ...");
//! execution_context.record_explicit_metrics(&measures, &dimensions, topic);
//!
//! // For nested executions, create child context
//! let child_ctx = execution_context.metric_context().child(SourceType::Workflow, "nested");
//! ```

use opentelemetry::trace::TraceContextExt as _;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

use tracing::warn;

use super::storage::MetricStorage;
use super::types::{ContextItem, ContextType, MetricUsage, SemanticContextItem, SourceType};

/// Shared metric context type for use in ExecutionContext
pub type SharedMetricCtx = Arc<RwLock<MetricContext>>;

/// Metric collection context that can be passed through ExecutionContext.
///
/// Unlike the thread-local approach, this works correctly with:
/// - `tokio::spawn` (context is cloned/moved into spawned task)
/// - Nested agent/workflow executions (child contexts link to parent)
/// - Cross-thread execution
///
/// ## Example
///
/// ```rust,ignore
/// // Create root context
/// let ctx = MetricContext::new(SourceType::Agent, "my-agent");
///
/// // Create child for nested execution
/// let child = ctx.child(SourceType::Workflow, "nested-workflow");
///
/// // Record data
/// ctx.lock().add_sql("SELECT ...");
///
/// // Finalize when done
/// ctx.finalize();
/// ```
#[derive(Debug, Clone)]
pub struct MetricContext {
    /// OpenTelemetry trace ID for correlation
    pub trace_id: String,
    /// Parent's trace ID (for nested execution correlation)
    pub parent_trace_id: Option<String>,
    /// Depth in execution tree (0 = root)
    pub depth: u32,
    /// Source type (Agent, Workflow, Task)
    pub source_type: SourceType,
    /// Reference to the source (e.g., agent_ref, workflow_ref)
    pub source_ref: String,

    /// User's question/prompt (set at input)
    question: Option<String>,
    /// Agent/workflow response (set at output)
    response: Option<String>,
    /// Accumulated SQL queries executed during this trace
    executed_sqls: Vec<String>,
    /// Explicit metrics from semantic query params, grouped by topic
    explicit_metrics: Vec<SemanticContextItem>,
}

impl MetricContext {
    /// Create a new root metric context
    pub fn new(source_type: SourceType, source_ref: impl Into<String>) -> Self {
        // Capture trace_id at creation for correlation
        let trace_id = current_trace_id();
        let source_ref = source_ref.into();
        tracing::debug!(
            "MetricContext::new created with trace_id={} for {}::{}",
            trace_id,
            source_type,
            source_ref
        );
        Self {
            trace_id,
            parent_trace_id: None,
            depth: 0,
            source_type,
            source_ref,
            question: None,
            response: None,
            executed_sqls: Vec::new(),
            explicit_metrics: Vec::new(),
        }
    }

    /// Create a child context linked to this parent
    ///
    /// Used when a nested agent/workflow is called.
    pub fn child(&self, source_type: SourceType, source_ref: impl Into<String>) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            parent_trace_id: Some(self.trace_id.clone()),
            depth: self.depth + 1,
            source_type,
            source_ref: source_ref.into(),
            question: None,
            response: None,
            executed_sqls: Vec::new(),
            explicit_metrics: Vec::new(),
        }
    }

    /// Create a shared (Arc<RwLock>) version of this context
    pub fn shared(self) -> SharedMetricCtx {
        Arc::new(RwLock::new(self))
    }

    /// Set the user's question/prompt
    pub fn set_question(&mut self, question: impl Into<String>) {
        self.question = Some(question.into());
    }

    /// Set the response
    pub fn set_response(&mut self, response: impl Into<String>) {
        self.response = Some(response.into());
    }

    /// Add an executed SQL query
    pub fn add_sql(&mut self, sql: impl Into<String>) {
        self.executed_sqls.push(sql.into());
    }

    /// Add explicit metrics from semantic query parameters
    /// Groups measures and dimensions by topic
    pub fn add_explicit_metrics(
        &mut self,
        measures: &[String],
        dimensions: &[String],
        topic: Option<&str>,
    ) {
        let topic = topic.map(|s| s.to_string());

        // Check if we already have this topic group
        if let Some(existing) = self.explicit_metrics.iter_mut().find(|m| m.topic == topic) {
            existing.measures.extend(measures.iter().cloned());
            existing.dimensions.extend(dimensions.iter().cloned());
        } else {
            // Add new topic group
            self.explicit_metrics.push(SemanticContextItem {
                topic,
                measures: measures.to_vec(),
                dimensions: dimensions.to_vec(),
            });
        }
    }

    /// Check if there's any data to process
    pub fn has_data(&self) -> bool {
        !self.explicit_metrics.is_empty()
            || !self.executed_sqls.is_empty()
            || self.question.is_some()
            || self.response.is_some()
    }

    /// Check if there's data for Tier 2 (LLM) extraction
    pub fn has_tier2_data(&self) -> bool {
        self.question.is_some() || self.response.is_some() || !self.executed_sqls.is_empty()
    }

    /// Collect all unique metric names from explicit metrics (Tier 1)
    fn collect_tier1_metrics(&self) -> HashSet<String> {
        let mut metrics = HashSet::new();
        for group in &self.explicit_metrics {
            metrics.extend(group.measures.iter().cloned());
            metrics.extend(group.dimensions.iter().cloned());
        }
        metrics
    }

    /// Build full context JSON with all available data
    fn build_full_context(&self) -> (Vec<ContextType>, Option<String>) {
        let mut context_items = Vec::new();
        let mut context_types = Vec::new();

        // Add question context
        if let Some(q) = &self.question {
            context_items.push(ContextItem {
                context_type: "question".to_string(),
                content: serde_json::json!(q),
            });
            context_types.push(ContextType::Question);
        }

        // Add response context
        if let Some(r) = &self.response {
            context_items.push(ContextItem {
                context_type: "response".to_string(),
                content: serde_json::json!(r),
            });
            context_types.push(ContextType::Response);
        }

        // Add SQL contexts
        for sql in &self.executed_sqls {
            context_items.push(ContextItem {
                context_type: "sql".to_string(),
                content: serde_json::json!(sql),
            });
            if !context_types.contains(&ContextType::SQL) {
                context_types.push(ContextType::SQL);
            }
        }

        // Add semantic context: already grouped by topic
        if !self.explicit_metrics.is_empty() {
            context_types.push(ContextType::SemanticQuery);
            context_items.push(ContextItem {
                context_type: "semantic".to_string(),
                content: serde_json::to_value(&self.explicit_metrics)
                    .unwrap_or(serde_json::json!([])),
            });
        }

        let context_json = if !context_items.is_empty() {
            serde_json::to_string(&context_items).ok()
        } else {
            None
        };

        (context_types, context_json)
    }

    /// Create MetricUsage records for given metric names
    fn create_metric_usages(
        &self,
        metric_names: &HashSet<String>,
        context_types: Vec<ContextType>,
        context: Option<String>,
    ) -> Vec<MetricUsage> {
        metric_names
            .iter()
            .map(|name| MetricUsage {
                metric_name: name.clone(),
                source_type: self.source_type,
                source_ref: self.source_ref.clone(),
                context_types: context_types.clone(),
                trace_id: self.trace_id.clone(),
                context: context.clone(),
            })
            .collect()
    }

    /// Finalize the context: store Tier 1 metrics and trigger Tier 2 extraction
    ///
    /// This spawns async tasks for:
    /// 1. Storing explicit metrics (Tier 1) from semantic queries
    /// 2. Triggering LLM extraction (Tier 2) from question/response/SQL
    pub fn finalize(self) {
        if !self.has_data() {
            tracing::debug!(
                "MetricContext finalize: no data to process (trace_id={}, source={}::{})",
                self.trace_id,
                self.source_type,
                self.source_ref
            );
            return;
        }

        let storage = Arc::new(MetricStorage::from_env());
        self.finalize_with_storage(storage);
    }

    /// Finalize with a specific storage instance
    pub fn finalize_with_storage(self, storage: Arc<MetricStorage>) {
        // Capture the current trace_id at finalization time, which may differ from creation
        let finalize_trace_id = current_trace_id();

        tracing::debug!(
            "MetricContext finalize: processing data (creation_trace_id={}, current_trace_id={}, source={}::{}, has_question={}, has_response={}, sql_count={}, explicit_metrics={})",
            self.trace_id,
            finalize_trace_id,
            self.source_type,
            self.source_ref,
            self.question.is_some(),
            self.response.is_some(),
            self.executed_sqls.len(),
            self.explicit_metrics.len()
        );

        // Use the current trace_id for storage, not the creation-time trace_id
        let trace_id = finalize_trace_id;
        let source_type = self.source_type;
        let source_ref = self.source_ref.clone();

        tokio::spawn(async move {
            tracing::info!(
                "🔵 MetricContext finalize task started (trace_id={}, source={}::{})",
                trace_id,
                source_type,
                source_ref
            );

            // Build full context once
            let (context_types, context_json) = self.build_full_context();

            // Collect Tier 1 metrics (explicit from semantic queries)
            let mut all_metrics = self.collect_tier1_metrics();
            tracing::info!(
                "📊 Collected {} Tier 1 metrics for trace_id={}",
                all_metrics.len(),
                trace_id
            );

            // Tier 2: LLM extraction from question/response/SQL
            let has_openai_key = std::env::var("OPENAI_API_KEY").is_ok();

            tracing::info!(
                "🔍 Tier 2 check for trace_id={}: OPENAI_API_KEY={}, question={}, response={}, sql_count={}",
                trace_id,
                has_openai_key,
                self.question.is_some(),
                self.response.is_some(),
                self.executed_sqls.len()
            );

            if has_openai_key && self.has_tier2_data() {
                tracing::info!("🚀 Running Tier 2 LLM extraction for trace_id={}", trace_id);

                let tier2_metrics = run_tier2_extraction(
                    self.question.as_deref(),
                    self.response.as_deref(),
                    &self.executed_sqls,
                )
                .await;

                tracing::info!(
                    "📊 Collected {} Tier 2 metrics for trace_id={}",
                    tier2_metrics.len(),
                    trace_id
                );

                // Merge Tier 2 into all_metrics (dedup automatically by HashSet)
                all_metrics.extend(tier2_metrics);
            } else {
                tracing::info!(
                    "⏭️  Skipping Tier 2 extraction for trace_id={} (OPENAI_API_KEY={}, has_tier2_data={})",
                    trace_id,
                    has_openai_key,
                    self.has_tier2_data()
                );
            }

            // Create and store MetricUsage for each unique metric
            if !all_metrics.is_empty() {
                let usages = self.create_metric_usages(&all_metrics, context_types, context_json);
                tracing::info!(
                    "💾 Storing {} unique metric usages for trace_id={}",
                    usages.len(),
                    trace_id
                );

                match storage.store_metrics(&usages).await {
                    Ok(_) => {
                        tracing::info!(
                            "✅ Successfully stored {} metrics for trace_id={}",
                            usages.len(),
                            trace_id
                        );
                    }
                    Err(e) => {
                        warn!(
                            "❌ Failed to store metrics for trace_id={}: {}",
                            trace_id, e
                        );
                    }
                }
            } else {
                tracing::info!("ℹ️  No metrics to store for trace_id={}", trace_id);
            }

            tracing::info!(
                "🔵 MetricContext finalize task completed (trace_id={})",
                trace_id
            );
        });
    }
}

/// Run Tier 2 LLM extraction and return extracted metric names
async fn run_tier2_extraction(
    question: Option<&str>,
    response: Option<&str>,
    executed_sqls: &[String],
) -> HashSet<String> {
    use super::extractor::{ExtractionContext, ExtractorConfig, MetricExtractor};

    let mut ctx = ExtractionContext::new();
    if let Some(q) = question {
        ctx = ctx.with_question(q.to_string());
    }
    if let Some(r) = response {
        ctx = ctx.with_response(r.to_string());
    }
    for sql in executed_sqls {
        ctx = ctx.with_sql(sql.clone());
    }

    let config = ExtractorConfig::from_env();
    match MetricExtractor::new(&config) {
        Ok(extractor) => match extractor.extract(&ctx).await {
            Ok(result) => {
                tracing::info!(
                    "✅ Extracted {} metrics via LLM: {:?}",
                    result.metrics.len(),
                    result.metric_names()
                );
                result.metrics.iter().map(|m| m.name.clone()).collect()
            }
            Err(e) => {
                warn!("❌ Metric extraction failed: {}", e);
                HashSet::new()
            }
        },
        Err(e) => {
            warn!("❌ Failed to create metric extractor: {}", e);
            HashSet::new()
        }
    }
}

/// Get the current trace ID from OpenTelemetry context
pub fn current_trace_id() -> String {
    tracing::Span::current()
        .context()
        .span()
        .span_context()
        .trace_id()
        .to_string()
}
