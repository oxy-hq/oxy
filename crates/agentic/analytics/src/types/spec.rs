//! [`QuerySpec`], [`AnalyticsSolution`], [`AnalyticsResult`], and visualisation types.

use std::path::PathBuf;

use agentic_core::result::QueryResult;
use serde::{Deserialize, Serialize};

use crate::catalog::QueryContext;

use super::intent::AnalyticsIntent;
use super::query_request::QueryRequestItem;

// ---------------------------------------------------------------------------
// Spec
// ---------------------------------------------------------------------------

/// The shape of the result set the query is expected to return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultShape {
    /// A single scalar value.
    Scalar,
    /// A one-dimensional list of values (one column, any number of rows).
    Series,
    /// A two-dimensional table with named columns.
    Table { columns: Vec<String> },
    /// A time-indexed series (≥ 2 columns, ≥ 2 rows).
    TimeSeries,
}

impl Default for ResultShape {
    fn default() -> Self {
        ResultShape::Table { columns: vec![] }
    }
}

impl std::fmt::Display for ResultShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResultShape::Scalar => write!(
                f,
                "Scalar (exactly 1 row, 1 column — e.g. SELECT COUNT(*) FROM t)"
            ),
            ResultShape::Series => write!(
                f,
                "Series (exactly 1 column, any number of rows — \
                 the SELECT must contain only ONE expression with NO extra columns; \
                 do NOT include date, group-by dimensions, or any second column)"
            ),
            ResultShape::Table { columns } if columns.is_empty() => {
                write!(f, "Table (2+ columns, any number of rows)")
            }
            ResultShape::Table { columns } => {
                write!(f, "Table with columns [{}]", columns.join(", "))
            }
            ResultShape::TimeSeries => write!(
                f,
                "TimeSeries (2+ columns including a date/time column, 2+ rows)"
            ),
        }
    }
}

/// A fully resolved, structured description of an analytics query ready to be
/// translated into a concrete execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySpec {
    /// The clarified intent this spec was produced from.
    pub intent: AnalyticsIntent,
    /// Resolved metric expressions in `"table.column"` form.
    pub resolved_metrics: Vec<String>,
    /// Resolved filter expressions with fully-qualified `table.column` references.
    ///
    /// Produced by the Specify stage (parallel to `resolved_metrics`) so that
    /// the Solve stage can directly emit a WHERE clause without having to
    /// re-discover which table each filter column belongs to.
    /// e.g. `["orders.created_at >= '2024-01-01'", "orders.status = 'active'"]`
    pub resolved_filters: Vec<String>,
    /// Tables required to answer the query.
    pub resolved_tables: Vec<String>,
    /// Ordered list of `(left_table, right_table, join_key)` pairs.
    pub join_path: Vec<(String, String, String)>,
    /// Expected shape of the result set.
    pub expected_result_shape: ResultShape,
    /// Assumptions made during resolution (surfaced to the user).
    pub assumptions: Vec<String>,
    /// Which path produced this spec — set by the Specify handler for
    /// path-aware diagnosis downstream.
    pub solution_source: SolutionSource,
    /// Pre-computed execution payload produced during Specifying.
    ///
    /// - `Some(SolutionPayload::Sql(sql))` — semantic layer compiled SQL; Solving is skipped.
    /// - `Some(SolutionPayload::Vendor(vq))` — vendor engine translated query; Solving is skipped.
    /// - `None` — Solving stage runs to generate SQL.
    ///
    /// Not serialized — this is transient pipeline state, never persisted.
    #[serde(skip)]
    pub precomputed: Option<SolutionPayload>,
    /// Rich semantic context produced by the catalog during Specifying.
    ///
    /// Populated on the `LlmWithSemanticContext` path so the Solving prompt
    /// has metric definitions, dimension summaries, join paths, and schema
    /// context without re-querying the catalog.
    pub context: Option<QueryContext>,
    /// Logical name of the connector that should execute the SQL for this spec.
    ///
    /// Set by the Specifying handler via schema-based routing: the first table
    /// in `resolved_tables` is looked up in `SchemaCatalog::connector_for_table`
    /// and the result is stored here.  Defaults to the solver's
    /// `default_connector` when no connector tag is found.  Propagated to
    /// `AnalyticsSolution.connector_name` so the Executing handler can route
    /// without re-inspecting the catalog.
    pub connector_name: String,
    /// The original airlayer-native query request item produced by the LLM.
    ///
    /// Preserved for cross-turn follow-ups and back-edge retry hints.
    /// Unlike `query_request` (the compiled form), this retains the LLM's
    /// original string-based operators and is serializable.
    #[serde(default)]
    pub query_request_item: Option<QueryRequestItem>,
    /// The airlayer-native query request produced by the LLM during Specifying.
    ///
    /// Populated on the airlayer-native path where the LLM produces a
    /// structured `QueryRequest` instead of SQL fragments.  When
    /// `engine.compile_query` succeeds in Specifying, `precomputed` is set
    /// and Solve is skipped.  When it fails non-retryably, `precomputed`
    /// stays `None` and the Solving stage handles the fallback.
    #[serde(skip)]
    pub query_request: Option<airlayer::engine::query::QueryRequest>,
    /// Error message from a failed airlayer compile attempt in Specifying.
    ///
    /// Set when the LLM produced a valid `QueryRequest` but
    /// `engine.compile_query` failed non-retryably.  Carried forward to
    /// Solving so it can include the error in the LLM prompt context and
    /// call `translate_to_raw_context` itself.
    #[serde(skip)]
    pub compile_error: Option<String>,
}
// ---------------------------------------------------------------------------
// Domain marker
// ---------------------------------------------------------------------------

/// Which path produced the [`QuerySpec`] — used for path-aware diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SolutionSource {
    /// The spec was produced by the semantic layer (fast, structured path).
    SemanticLayer,
    /// The spec was produced by an LLM call (fallback / complex-query path).
    #[default]
    LlmWithSemanticContext,
    /// The LLM generated a multi-step `procedure.yml` during Specifying.
    ///
    /// `file_path` points to the generated file on disk.  The Solving stage
    /// is skipped entirely; the Executing stage delegates to the external
    /// [`ProcedureRunner`](crate::procedure::ProcedureRunner).
    Procedure { file_path: PathBuf },
    /// A vendor semantic engine translated and executed the query natively.
    ///
    /// The string is the value returned by `SemanticEngine::vendor_name()` at
    /// translation time. Used for telemetry and diagnostics only.
    /// The Solving stage is skipped; the Executing stage calls the engine.
    VendorEngine(String),
}

/// The execution payload pre-computed by the Specify stage.
///
/// `None` on `QuerySpec.precomputed` means "enter Solving"; `Some` means
/// "skip Solving — this payload is ready for Executing".
#[derive(Debug, Clone)]
pub enum SolutionPayload {
    /// Standard SQL string, executed via the database connector.
    Sql(String),
    /// Vendor-native query, executed via [`SemanticEngine::execute`][crate::engine::SemanticEngine].
    Vendor(crate::engine::VendorQuery),
}

impl SolutionPayload {
    /// Return the SQL string if this is the `Sql` variant.
    pub fn sql(&self) -> Option<&str> {
        match self {
            SolutionPayload::Sql(s) => Some(s),
            SolutionPayload::Vendor(_) => None,
        }
    }

    /// Return the SQL string, panicking if this is the `Vendor` variant.
    ///
    /// Use only in contexts where the payload is statically known to be SQL.
    pub fn expect_sql(&self) -> &str {
        match self {
            SolutionPayload::Sql(s) => s,
            SolutionPayload::Vendor(_) => {
                panic!("expected SolutionPayload::Sql but got Vendor")
            }
        }
    }
}

/// The query produced by the Specify or Solve stage, ready for Executing.
#[derive(Clone)]
pub struct AnalyticsSolution {
    /// The execution payload — SQL string or vendor-native query.
    pub payload: SolutionPayload,
    /// Which path produced the spec that this solution was generated from.
    pub solution_source: SolutionSource,
    /// Logical name of the connector that should execute this SQL.
    ///
    /// Propagated from `QuerySpec.connector_name` by the Solving stage
    /// (and the Solving-skip boundary).  The Executing handler routes to
    /// `AnalyticsSolver::connectors[connector_name]`.
    pub connector_name: String,
    /// Structured semantic query (dimensions, measures, filters, time dimensions)
    /// that produced this solution.  Populated on the `SolutionSource::SemanticLayer`
    /// path and forwarded on the `QueryExecuted` event so the UI can render the
    /// semantic view alongside the compiled SQL for verified queries.  `None` on
    /// all other paths.
    pub semantic_query: Option<QueryRequestItem>,
}

/// One executed-query result set (data + optional column stats).
#[derive(Debug, Clone)]
pub struct QueryResultSet {
    /// Rows and column metadata returned by the executed query.
    pub data: QueryResult,
    /// Per-column statistics from the connector. `None` for test fixtures
    /// or when the connector doesn't compute stats.
    pub summary: Option<agentic_connector::ResultSummary>,
}

/// The raw query results produced by the Execute stage.
///
/// Holds a single `QueryResultSet` for normal (single-spec) queries and
/// multiple sets when the Specifying stage produced a fan-out.  The
/// Interpreting stage receives all result sets directly and reasons over
/// them without the artificial merging that the old `merge_results` approach
/// required.
#[derive(Debug, Clone)]
pub struct AnalyticsResult {
    /// One entry per executed spec.  Always has at least one element.
    pub results: Vec<QueryResultSet>,
}

impl AnalyticsResult {
    /// Construct a single-result [`AnalyticsResult`] from one executed query.
    pub fn single(data: QueryResult, summary: Option<agentic_connector::ResultSummary>) -> Self {
        Self {
            results: vec![QueryResultSet { data, summary }],
        }
    }

    /// Returns `true` when this result came from a multi-spec fan-out.
    pub fn is_multi(&self) -> bool {
        self.results.len() > 1
    }

    /// Borrow the primary (first) result set.
    ///
    /// Used by single-result paths (executing validator, compact formatter).
    /// Panics if `results` is empty — callers must guarantee at least one entry.
    pub fn primary(&self) -> &QueryResultSet {
        self.results
            .first()
            .expect("AnalyticsResult must have at least one result set")
    }
}

/// Chart configuration produced by the Interpret stage when the LLM decides
/// a visualisation would help answer the question.
///
/// Column names must match actual columns in the query result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartConfig {
    /// Chart variant: `"line_chart"`, `"bar_chart"`, `"pie_chart"`, or `"table"`.
    pub chart_type: String,
    /// X-axis column name (line / bar charts).
    pub x: Option<String>,
    /// Y-axis column name (line / bar charts).
    pub y: Option<String>,
    /// Optional grouping / series column (line / bar charts).
    pub series: Option<String>,
    /// Category column name (pie charts).
    pub name: Option<String>,
    /// Value column name (pie charts).
    pub value: Option<String>,
    /// Optional chart title.
    pub title: Option<String>,
    /// Optional x-axis label.
    pub x_axis_label: Option<String>,
    /// Optional y-axis label.
    pub y_axis_label: Option<String>,
}

/// A self-contained display block produced by the Interpret stage.
///
/// Carries both the chart/table configuration *and* the data it should render,
/// so the frontend can hydrate and display it without an additional round-trip.
#[derive(Debug, Clone)]
pub struct DisplayBlock {
    /// How to render the data.
    pub config: ChartConfig,
    /// Column names from the query result.
    pub columns: Vec<String>,
    /// Row data as JSON values (parallel to `columns`).
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// The natural-language answer produced by the Interpret stage.
#[derive(Clone)]
pub struct AnalyticsAnswer {
    /// Human-readable answer to the original question.
    pub text: String,
    /// Zero or more charts / tables to display alongside the text answer.
    ///
    /// Empty for scalar answers, general-inquiry responses, or when the LLM
    /// decides no visualization is needed.  Contains one entry per
    /// `render_chart` tool call — each call emits a `ChartRendered` event
    /// immediately mid-stream; the collected configs are stored here so the
    /// orchestrator can inspect them after the run.
    pub display_blocks: Vec<DisplayBlock>,
    /// The airlayer query structure from the Specifying stage, if any.
    ///
    /// Carried through to the HTTP layer so it can be persisted for cross-turn
    /// follow-up continuity.
    pub spec_hint: Option<QueryRequestItem>,
}
