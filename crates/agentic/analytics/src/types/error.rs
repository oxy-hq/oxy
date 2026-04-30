//! [`AnalyticsError`] — domain-specific errors raised at any pipeline stage.

use super::spec::ResultShape;

/// Domain-specific errors that can arise at any pipeline stage.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalyticsError {
    /// A metric name could not be resolved to any known column.
    UnresolvedMetric { metric: String },
    /// A column name matches more than one table and cannot be disambiguated.
    AmbiguousColumn { column: String, tables: Vec<String> },
    /// A join path references a table or key that does not exist in the schema.
    UnresolvedJoin {
        left: String,
        right: String,
        key: String,
        reason: String,
    },
    /// The generated or supplied query has a syntax error.
    SyntaxError { query: String, message: String },
    /// The query executed successfully but returned no rows.
    EmptyResults { query: String },
    /// The result set's shape does not match the expected shape.
    ShapeMismatch {
        expected: ResultShape,
        actual: ResultShape,
    },
    /// A value in the result set is outside the expected range or is
    /// statistically anomalous.
    ValueAnomaly {
        column: String,
        value: String,
        reason: String,
    },
    /// The pipeline cannot proceed without additional input from the user.
    NeedsUserInput { prompt: String },
    /// The airlayer compiler returned an error when trying to compile the LLM-produced `QueryRequest`.
    AirlayerCompileError { error_message: String },
    /// The chart config produced by the Interpret stage references columns
    /// that do not exist in the query result.
    InvalidChartConfig { errors: Vec<String> },
    /// A vendor semantic engine returned an error during query execution.
    ///
    /// Covers both API-level errors (HTTP 4xx/5xx with a body) and transport
    /// failures (network, serialisation).
    VendorError {
        vendor_name: String,
        message: String,
    },
    /// The LLM returned a rate-limit (429) response; the solver will retry with
    /// exponential backoff.  This variant routes back to `Solving` so the retry
    /// attempt counter is independent of the general transient-error budget.
    RateLimitRetry(String),
}

impl std::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyticsError::UnresolvedMetric { metric } => {
                write!(
                    f,
                    "unresolved metric or column: '{metric}' — check the schema and use a fully-qualified table.column reference"
                )
            }
            AnalyticsError::AmbiguousColumn { column, tables } => {
                write!(
                    f,
                    "column '{column}' is ambiguous — it appears in tables: {}; qualify it as table.column",
                    tables.join(", ")
                )
            }
            AnalyticsError::UnresolvedJoin {
                left,
                right,
                key,
                reason,
            } => {
                write!(
                    f,
                    "join path error: {reason} (joining `{left}` to `{right}` on key `{key}`)"
                )
            }
            AnalyticsError::SyntaxError { message, .. } => {
                write!(f, "SQL syntax error: {message}")
            }
            AnalyticsError::EmptyResults { .. } => {
                write!(
                    f,
                    "query returned no rows — try relaxing filters or broadening the time range"
                )
            }
            AnalyticsError::ShapeMismatch { expected, actual } => {
                write!(
                    f,
                    "result shape mismatch: expected {expected} but got {actual}. \
                     FIX: rewrite the SELECT clause to match the expected shape. \
                     For TimeSeries: SELECT a date/time column FIRST, then one or more value columns (e.g. SELECT date, COUNT(*) AS n FROM t GROUP BY date ORDER BY date). \
                     For Series: SELECT only ONE column/expression (no date, no GROUP BY dimensions). \
                     For Scalar: SELECT exactly one aggregate with no GROUP BY. \
                     For Table: include all required columns."
                )
            }
            AnalyticsError::ValueAnomaly {
                column,
                value,
                reason,
            } => {
                write!(f, "value anomaly in column '{column}': {value} — {reason}")
            }
            AnalyticsError::NeedsUserInput { prompt } => {
                write!(f, "needs user input: {prompt}")
            }
            AnalyticsError::AirlayerCompileError { error_message } => {
                write!(f, "airlayer compile error: {error_message}")
            }
            AnalyticsError::InvalidChartConfig { errors } => {
                write!(
                    f,
                    "chart config references invalid columns: {}",
                    errors.join("; ")
                )
            }
            AnalyticsError::VendorError {
                vendor_name,
                message,
            } => {
                write!(f, "vendor engine '{vendor_name}' error: {message}")
            }
            AnalyticsError::RateLimitRetry(msg) => {
                write!(f, "rate limit exceeded (retrying): {msg}")
            }
        }
    }
}
