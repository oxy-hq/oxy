//! Semantic-query configuration types.
//!
//! These are the subset of YAML config types the compile path needs. They
//! were originally defined in `agentic_automation::config`; lifting them into
//! this crate keeps the automation crate free of a back-edge dependency once
//! the analytics domain also calls `resolve_and_compile`.
//!
//! `agentic-automation::config` re-exports the same names so existing call
//! sites and YAML round-trips are unchanged.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Semantic query parameters — the subset of fields that the semantic compiler
/// needs. Mirrors `oxy::types::SemanticQueryParams` but self-contained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticQueryConfig {
    pub topic: Option<String>,
    #[serde(default)]
    pub measures: Vec<String>,
    #[serde(default)]
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub time_dimensions: Vec<TimeDimensionConfig>,
    #[serde(default)]
    pub filters: Vec<SemanticFilter>,
    #[serde(default, alias = "order")]
    pub orders: Vec<SemanticOrder>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeDimensionConfig {
    pub dimension: String,
    pub granularity: Option<TimeGranularity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticFilter {
    pub field: String,
    #[serde(flatten)]
    pub filter_type: SemanticFilterType,
}

/// Filter operators for semantic queries. Mirrors `oxy::config::model::SemanticFilterType`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum SemanticFilterType {
    #[serde(rename = "eq")]
    Eq(ScalarFilter),
    #[serde(rename = "neq")]
    Neq(ScalarFilter),
    #[serde(rename = "gt")]
    Gt(ScalarFilter),
    #[serde(rename = "gte")]
    Gte(ScalarFilter),
    #[serde(rename = "lt")]
    Lt(ScalarFilter),
    #[serde(rename = "lte")]
    Lte(ScalarFilter),
    #[serde(rename = "in")]
    In(ArrayFilter),
    #[serde(rename = "not_in")]
    NotIn(ArrayFilter),
    #[serde(rename = "in_date_range")]
    InDateRange(DateRangeFilter),
    #[serde(rename = "not_in_date_range")]
    NotInDateRange(DateRangeFilter),
    #[serde(rename = "contains")]
    Contains(ScalarFilter),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalarFilter {
    pub value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrayFilter {
    pub values: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRangeFilter {
    pub from: Value,
    pub to: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticOrder {
    pub field: String,
    #[serde(default = "default_asc")]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeGranularity {
    Year,
    Quarter,
    Month,
    Week,
    Day,
    Hour,
    Minute,
    Second,
}

fn default_asc() -> String {
    "asc".to_string()
}
