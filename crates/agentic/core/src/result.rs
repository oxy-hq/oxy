//! Query result types: `CellValue`, `QueryRow`, `QueryResult`.

/// A single cell value in a query result.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// A text / string value.
    Text(String),
    /// A numeric value (integer or floating-point, stored as `f64`).
    Number(f64),
    /// SQL `NULL`.
    Null,
}

/// A single row in a query result.
#[derive(Debug, Clone)]
pub struct QueryRow(pub Vec<CellValue>);

/// The result of executing an analytics query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Column names in the same order as the cell values in each row.
    pub columns: Vec<String>,
    /// Bounded sample of rows (capped by the connector's `sample_limit`).
    pub rows: Vec<QueryRow>,
    /// Actual total row count in the full result set (may be > rows.len()).
    /// Set by the connector. When no connector is used (e.g. test fixtures),
    /// set this to `rows.len() as u64`.
    pub total_row_count: u64,
    /// `true` when `rows.len() < total_row_count` (result was capped).
    pub truncated: bool,
}
