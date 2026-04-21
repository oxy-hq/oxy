//! Airlayer-native query request types consumed by the Specifying LLM response.

use serde::{Deserialize, Serialize};

/// Confirmed query structure from a prior Specifying attempt.
///
/// Uses the airlayer `QueryRequestItem` grammar (measures, dimensions,
/// filters, time_dimensions, order, limit) so the LLM can reuse the
/// prior query structure on back-edge retries and cross-turn follow-ups.
pub type SpecHint = QueryRequestItem;

// ---------------------------------------------------------------------------
// Airlayer-native query request types (LLM response deserialization)
// ---------------------------------------------------------------------------

/// Top-level envelope for the airlayer-native Specify response.
///
/// The LLM returns one or more `QueryRequestItem` specs, each of which can
/// be independently compiled via `airlayer::SemanticEngine::compile_query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequestEnvelope {
    pub specs: Vec<QueryRequestItem>,
}

/// A single query spec in airlayer-native format.
///
/// Mirrors `airlayer::engine::query::QueryRequest` but includes an
/// `assumptions` field for human review and uses owned deserialization types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryRequestItem {
    /// Measure members to aggregate (e.g. `["orders.total_revenue"]`).
    #[serde(default)]
    pub measures: Vec<String>,
    /// Non-time dimension members to group by (e.g. `["orders.status"]`).
    #[serde(default)]
    pub dimensions: Vec<String>,
    /// Structured filter conditions.
    #[serde(default)]
    pub filters: Vec<StructuredFilter>,
    /// Time dimensions with granularity and optional date range.
    #[serde(default)]
    pub time_dimensions: Vec<TimeDimensionItem>,
    /// Sort order.
    #[serde(default)]
    pub order: Vec<OrderItem>,
    /// Row limit (null for no limit).
    pub limit: Option<u64>,
    /// Assumptions made during resolution.
    #[serde(default)]
    pub assumptions: Vec<String>,
}

/// A structured filter condition from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredFilter {
    /// Member path in `view.member` format.
    pub member: String,
    /// Filter operator (camelCase, matching airlayer's `FilterOperator`).
    pub operator: String,
    /// Filter values as strings.
    #[serde(default)]
    pub values: Vec<String>,
}

/// A time dimension entry from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeDimensionItem {
    /// Time dimension member in `view.member` format.
    pub dimension: String,
    /// Granularity (e.g. "month", "day") or null.
    pub granularity: Option<String>,
    /// Date range as `[start, end]` or null.
    pub date_range: Option<Vec<String>>,
}

/// An order-by entry from the LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    /// Member to order by in `view.member` format.
    pub id: String,
    /// True for descending.
    #[serde(default)]
    pub desc: bool,
}

impl QueryRequestItem {
    /// Convert to an airlayer `QueryRequest` for compilation.
    pub fn to_query_request(&self) -> airlayer::engine::query::QueryRequest {
        use airlayer::engine::query::{OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery};

        let filters = self
            .filters
            .iter()
            .map(|f| QueryFilter {
                member: Some(f.member.clone()),
                operator: Some(parse_filter_operator(&f.operator)),
                values: f.values.clone(),
                and: None,
                or: None,
            })
            .collect();

        let time_dimensions = self
            .time_dimensions
            .iter()
            .map(|td| TimeDimensionQuery {
                dimension: td.dimension.clone(),
                granularity: td.granularity.clone(),
                date_range: td.date_range.clone(),
            })
            .collect();

        let order = self
            .order
            .iter()
            .map(|o| OrderBy {
                id: o.id.clone(),
                desc: o.desc,
            })
            .collect();

        QueryRequest {
            measures: self.measures.clone(),
            dimensions: self.dimensions.clone(),
            filters,
            segments: vec![],
            time_dimensions,
            order,
            limit: self.limit,
            offset: None,
            timezone: None,
            ungrouped: false,
            through: vec![],
            motif: None,
            motif_params: Default::default(),
        }
    }
}

/// Parse a camelCase operator string into an airlayer `FilterOperator`.
fn parse_filter_operator(s: &str) -> airlayer::engine::query::FilterOperator {
    use airlayer::engine::query::FilterOperator;
    match s {
        "equals" => FilterOperator::Equals,
        "notEquals" => FilterOperator::NotEquals,
        "contains" => FilterOperator::Contains,
        "notContains" => FilterOperator::NotContains,
        "startsWith" => FilterOperator::StartsWith,
        "notStartsWith" => FilterOperator::NotStartsWith,
        "endsWith" => FilterOperator::EndsWith,
        "notEndsWith" => FilterOperator::NotEndsWith,
        "gt" => FilterOperator::Gt,
        "gte" => FilterOperator::Gte,
        "lt" => FilterOperator::Lt,
        "lte" => FilterOperator::Lte,
        "set" => FilterOperator::Set,
        "notSet" => FilterOperator::NotSet,
        "inDateRange" => FilterOperator::InDateRange,
        "notInDateRange" => FilterOperator::NotInDateRange,
        "beforeDate" => FilterOperator::BeforeDate,
        "beforeOrOnDate" => FilterOperator::BeforeOrOnDate,
        "afterDate" => FilterOperator::AfterDate,
        "afterOrOnDate" => FilterOperator::AfterOrOnDate,
        // Fallback for unknown operators
        _ => FilterOperator::Equals,
    }
}
