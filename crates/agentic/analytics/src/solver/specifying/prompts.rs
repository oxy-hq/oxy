//! Prompt builders for the Specifying stage.

use agentic_core::back_target::RetryContext;

use crate::AnalyticsIntent;
use crate::semantic::SemanticCatalog;

use super::super::prompts::{format_retry_section, format_spec_hint_section};

/// Format the triage-produced semantic query as a hint for the Specifying LLM.
///
/// When triage found relevant measures/dimensions (even at low confidence),
/// this injects them into the user prompt so Specifying doesn't have to
/// re-discover them from scratch via `search_catalog`.
pub(super) fn format_semantic_query_hint(intent: &AnalyticsIntent) -> String {
    let sq = &intent.semantic_query;
    if sq.measures.is_empty() && sq.dimensions.is_empty() && sq.time_dimensions.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!(
        "Triage semantic hint (confidence {:.2}):",
        intent.semantic_confidence
    )];
    if !sq.measures.is_empty() {
        parts.push(format!("  measures: {}", sq.measures.join(", ")));
    }
    if !sq.dimensions.is_empty() {
        parts.push(format!("  dimensions: {}", sq.dimensions.join(", ")));
    }
    if !sq.time_dimensions.is_empty() {
        let tds: Vec<String> = sq
            .time_dimensions
            .iter()
            .map(|td| {
                format!(
                    "{} ({})",
                    td.dimension,
                    td.granularity.as_deref().unwrap_or("auto")
                )
            })
            .collect();
        parts.push(format!("  time_dimensions: {}", tds.join(", ")));
    }
    if !sq.filters.is_empty() {
        let fs: Vec<String> = sq
            .filters
            .iter()
            .map(|f| format!("{} {} {:?}", f.member, f.operator, f.values))
            .collect();
        parts.push(format!("  filters: {}", fs.join("; ")));
    }
    format!("\n{}\n", parts.join("\n"))
}

pub(crate) fn build_specify_user_prompt(
    intent: &AnalyticsIntent,
    _catalog: &SemanticCatalog,
    retry_ctx: Option<&RetryContext>,
) -> String {
    let retry_section = format_retry_section(retry_ctx);
    let hint_section = format_spec_hint_section(intent.spec_hint.as_ref());
    let semantic_hint = format_semantic_query_hint(intent);
    format!(
        "Analytics Intent:\n\
         - Question: {raw_question}\n\
         - Summary: {summary}\n\
         - Question type: {question_type:?}\n\
         - Metrics: {metrics}\n\
         - Dimensions: {dimensions}\n\
         - Filters: {filters}\n\
         {semantic_hint}\n\
         Resolve the metrics and dimensions to concrete schema columns and return the JSON spec.\
         {hint_section}\
         {retry_section}",
        raw_question = intent.raw_question,
        summary = if intent.summary.is_empty() {
            "(none)".to_string()
        } else {
            intent.summary.clone()
        },
        question_type = intent.question_type,
        metrics = if intent.metrics.is_empty() {
            "(none)".to_string()
        } else {
            intent.metrics.join(", ")
        },
        dimensions = if intent.dimensions.is_empty() {
            "(none)".to_string()
        } else {
            intent.dimensions.join(", ")
        },
        filters = if intent.filters.is_empty() {
            "(none)".to_string()
        } else {
            intent.filters.join("; ")
        },
    )
}

/// Prompt builder for the airlayer-native QueryRequest specify path.
pub(super) fn build_specify_query_request_user_prompt(
    intent: &AnalyticsIntent,
    catalog: &SemanticCatalog,
    retry_ctx: Option<&RetryContext>,
) -> String {
    let retry_section = format_retry_section(retry_ctx);
    let hint_section = format_spec_hint_section(intent.spec_hint.as_ref());
    let semantic_hint = format_semantic_query_hint(intent);
    format!(
        "Analytics Intent:\n\
         - Question: {raw_question}\n\
         - Summary: {summary}\n\
         - Question type: {question_type:?}\n\
         - Metrics: {metrics}\n\
         - Dimensions: {dimensions}\n\
         - Filters: {filters}\n\
         {semantic_hint}\n\
         Semantic Catalog:\n{schema}\n\n\
         Map the intent to a structured query request using view.member references.\
         {hint_section}\
         {retry_section}",
        raw_question = intent.raw_question,
        summary = if intent.summary.is_empty() {
            "(none)".to_string()
        } else {
            intent.summary.clone()
        },
        question_type = intent.question_type,
        metrics = if intent.metrics.is_empty() {
            "(none)".to_string()
        } else {
            intent.metrics.join(", ")
        },
        dimensions = if intent.dimensions.is_empty() {
            "(none)".to_string()
        } else {
            intent.dimensions.join(", ")
        },
        filters = if intent.filters.is_empty() {
            "(none)".to_string()
        } else {
            intent.filters.join("; ")
        },
        schema = catalog.to_prompt_string(),
    )
}
