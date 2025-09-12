use std::collections::HashSet;

use crate::config::model::omni::OmniFilter;

#[derive(Debug, Clone)]
pub struct OmniExecutable;

#[derive(Debug, Clone)]
pub struct Filter {
    pub field: String,
    pub filter: OmniFilter,
}

#[derive(Debug, Clone)]
pub struct CompiledField {
    pub sql: String,
    pub required_views: HashSet<String>,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone)]
pub struct SqlParts {
    pub base_table: String,
    pub join_clauses: Vec<String>,
    pub select_clauses: Vec<String>,
    pub where_clauses: Vec<String>,
    pub order_clauses: Vec<String>,
    pub group_clauses: Vec<String>,
    pub having_clauses: Vec<String>,
    pub limit: Option<u64>,
}
