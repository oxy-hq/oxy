//! Core domain types: intent, spec, error, and domain marker.

use agentic_core::domain::Domain;

use crate::semantic::SemanticCatalog;

pub mod error;
pub mod intent;
pub mod query_request;
pub mod spec;

#[cfg(test)]
mod tests;

pub use error::AnalyticsError;
pub use intent::{
    AnalyticsIntent, ConversationTurn, DomainHypothesis, MissingMember, MissingMemberKind,
    QuestionType,
};
#[allow(unused_imports)]
pub use query_request::{
    OrderItem, QueryRequestEnvelope, QueryRequestItem, SpecHint, StructuredFilter,
    TimeDimensionItem,
};
pub use spec::{
    AnalyticsAnswer, AnalyticsResult, AnalyticsSolution, ChartConfig, DisplayBlock, QueryResultSet,
    QuerySpec, ResultShape, SolutionPayload, SolutionSource,
};

/// Type alias kept for backward compatibility.
///
/// New code should use [`SemanticCatalog`] directly.
pub type AnalyticsCatalog = SemanticCatalog;

/// Domain marker for the analytics pipeline.
pub struct AnalyticsDomain;

impl Domain for AnalyticsDomain {
    type Intent = AnalyticsIntent;
    type Spec = QuerySpec;
    type Solution = AnalyticsSolution;
    type Result = AnalyticsResult;
    type Answer = AnalyticsAnswer;
    /// The primary catalog type — combines semantic layer with raw schema.
    type Catalog = SemanticCatalog;
    type Error = AnalyticsError;
}
