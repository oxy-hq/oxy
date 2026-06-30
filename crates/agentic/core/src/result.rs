//! Query result types.
//!
//! Two parallel shapes coexist here:
//!
//! 1. [`CellValue`] + [`QueryResult`] — bounded, string-or-number rows used
//!    by the solver when sampling for the LLM. Loses type fidelity by design;
//!    the LLM only ever sees text representations.
//! 2. [`TypedValue`] + [`ColumnSpec`] + [`TypedRowStream`] — full, typed,
//!    streaming rows used by consumers that persist results (e.g. to Parquet)
//!    or render them in a typed data grid.

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_core::Stream;

// ── Bounded sample: CellValue / QueryRow / QueryResult ──────────────────────

/// A single cell value in a bounded sample result.
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// A text / string value.
    Text(String),
    /// A numeric value (integer or floating-point, stored as `f64`).
    Number(f64),
    /// SQL `NULL`.
    Null,
}

/// A single row in a bounded sample result.
#[derive(Debug, Clone)]
pub struct QueryRow(pub Vec<CellValue>);

/// The result of executing an analytics query (bounded sample).
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

// ── Full typed stream: TypedValue / ColumnSpec / TypedRowStream ──────────────

/// Logical type of a column in a [`TypedRowStream`].
///
/// Matches Arrow's logical categories so connectors can translate to the
/// corresponding `arrow::datatypes::DataType` without information loss.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedDataType {
    Bool,
    Int32,
    Int64,
    Float64,
    Text,
    Bytes,
    /// Calendar date (no time of day, no timezone).
    /// [`TypedValue::Date`] carries days since `1970-01-01`.
    Date,
    /// Instant in time (UTC, microsecond precision).
    /// [`TypedValue::Timestamp`] carries microseconds since the Unix epoch.
    Timestamp,
    /// Exact decimal with caller-defined precision / scale.
    /// [`TypedValue::Decimal`] carries the canonical string representation
    /// so callers preserve full precision without going through `f64`.
    Decimal {
        precision: u8,
        scale: i8,
    },
    /// Arbitrary JSON object or scalar.
    Json,
    /// The connector returned a native type outside the enumerated set.
    /// [`TypedValue`] will be `Text` with the driver's string rendering, or
    /// `Bytes` for binary driver types.
    Unknown,
}

/// Column metadata emitted alongside a [`TypedRowStream`].
#[derive(Debug, Clone)]
pub struct ColumnSpec {
    pub name: String,
    pub data_type: TypedDataType,
}

/// A single cell value in a full, typed row stream.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedValue {
    Null,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float64(f64),
    Text(String),
    Bytes(Vec<u8>),
    /// Days since `1970-01-01` (matches Arrow `Date32`).
    Date(i32),
    /// Microseconds since the Unix epoch, UTC
    /// (matches Arrow `Timestamp(Microsecond, Some("UTC"))`).
    Timestamp(i64),
    /// Decimal as its canonical string form (e.g. `"123.4500"`).
    /// The paired [`TypedDataType::Decimal`] carries precision / scale.
    Decimal(String),
    /// Arbitrary JSON value.
    Json(serde_json::Value),
}

/// `'static` boxed stream of typed row batches.
///
/// Each item is one full row — `Vec<TypedValue>` with the same length and
/// order as [`TypedRowStream::columns`]. The `'static` bound makes the
/// stream easy to forward through tokio tasks and HTTP handlers without
/// lifetime plumbing.
pub type BoxedRowStream =
    Pin<Box<dyn Stream<Item = Result<Vec<TypedValue>, TypedRowError>> + Send + 'static>>;

/// Errors emitted by a [`TypedRowStream`] mid-flight.
///
/// Kept structural — connectors translate their driver-specific errors into
/// these variants so consumers can handle them uniformly without depending on
/// any given driver.
#[derive(Debug, Clone)]
pub enum TypedRowError {
    /// The driver reported an error during row fetch or decoding.
    DriverError(String),
    /// The connector produced a value it could not map to [`TypedValue`].
    TypeMappingError {
        column: String,
        native_type: String,
        message: String,
    },
}

impl std::fmt::Display for TypedRowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DriverError(msg) => write!(f, "driver error: {msg}"),
            Self::TypeMappingError {
                column,
                native_type,
                message,
            } => write!(
                f,
                "type mapping error for column '{column}' (native type '{native_type}'): {message}"
            ),
        }
    }
}

impl std::error::Error for TypedRowError {}

/// Full, typed, streaming query result.
///
/// Unlike [`QueryResult`], this carries every row the query produces and
/// preserves native column types. Produced by
/// `DatabaseConnector::execute_query_full` and consumed by the Parquet
/// writer in the Dev Portal.
pub struct TypedRowStream {
    pub columns: Vec<ColumnSpec>,
    pub rows: BoxedRowStream,
    /// Set by the producer when it stopped reading early at the connector's
    /// [`ResultCap`](../../connector/src/connector.rs) memory backstop, i.e. the
    /// rows below are a *partial* result. Consumers (the Parquet writer) read
    /// this **after** fully draining `rows` and surface it as the response
    /// `truncated` flag. `None` means the backend does not signal truncation —
    /// treat as not truncated. Use [`truncation_flag_set`] to read it.
    pub truncated: Option<Arc<AtomicBool>>,
}

/// Read a producer truncation flag (as carried by [`TypedRowStream::truncated`]),
/// returning `false` when the flag is absent. Call this **after** the row stream
/// has been fully drained: streaming producers only set the flag once they hit
/// the cap mid-drain.
pub fn truncation_flag_set(flag: &Option<Arc<AtomicBool>>) -> bool {
    flag.as_ref().is_some_and(|f| f.load(Ordering::Relaxed))
}

impl std::fmt::Debug for TypedRowStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypedRowStream")
            .field("columns", &self.columns)
            .field("rows", &"<stream>")
            .finish()
    }
}

impl TypedRowStream {
    /// Build a stream by wrapping an in-memory collection of rows.
    ///
    /// Used by backends that eagerly materialize results (DuckDB, BigQuery's
    /// `ResultSet`) and don't have a natural cursor-style streaming API.
    pub fn from_rows(
        columns: Vec<ColumnSpec>,
        rows: Vec<Result<Vec<TypedValue>, TypedRowError>>,
    ) -> Self {
        let stream = async_stream::stream! {
            for row in rows {
                yield row;
            }
        };
        Self {
            columns,
            rows: Box::pin(stream),
            truncated: None,
        }
    }

    /// Attach a producer truncation flag, marking this stream as a *potentially*
    /// partial result. Eager (collect-then-stream) producers set the flag's
    /// value during collection and pass the same `Arc` here; streaming producers
    /// hand the flag to their stream adaptor, which sets it on overflow.
    pub fn with_truncation(mut self, flag: Arc<AtomicBool>) -> Self {
        self.truncated = Some(flag);
        self
    }
}
