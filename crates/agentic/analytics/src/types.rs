//! Core domain types: intent, spec, error, and domain marker.

use std::path::PathBuf;

use agentic_core::HumanInputQuestion;
use serde::{Deserialize, Serialize};

use crate::catalog::QueryContext;
use crate::semantic::SemanticCatalog;
use agentic_core::domain::Domain;
use agentic_core::result::QueryResult;

// ---------------------------------------------------------------------------
// Intent
// ---------------------------------------------------------------------------

/// The type of analytical question being asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    /// "How has X changed over time?"
    Trend,
    /// "How does X compare to Y?"
    Comparison,
    /// "What is X broken down by Y?"
    Breakdown,
    /// "What is the current value of X?"
    SingleValue,
    /// "How is X distributed?"
    Distribution,
    /// A general question that does not require a SQL query — e.g. "what tables
    /// do you have?", "what metrics can you track?", or any conversational
    /// follow-up that the system can answer directly from schema context.
    GeneralInquiry,
}

/// A single completed question–answer exchange, kept for follow-up context.
///
/// Passed in as part of [`AnalyticsIntent::history`] on subsequent questions
/// so that the Clarify and Interpret stages can reference prior exchanges when
/// resolving ambiguities or phrasing answers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationTurn {
    /// The natural-language question posed by the user.
    pub question: String,
    /// The natural-language answer produced by the Interpret stage.
    pub answer: String,
}

/// Confirmed schema resolutions from a prior Specifying attempt.
///
/// Populated on back-edges where a `QuerySpec` was produced successfully but
/// the attempt failed in validation or execution.  Injected into the retry
/// LLM prompt so the model reuses known-good resolutions rather than
/// re-discovering them from scratch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpecHint {
    /// SQL-level metric expressions confirmed in the prior attempt.
    /// e.g. `["SUM(fact_orders.gross_revenue)", "COUNT(fact_orders.id)"]`
    pub resolved_metrics: Vec<String>,
    /// Tables confirmed to be required by the prior attempt.
    pub resolved_tables: Vec<String>,
    /// Join triples: `(left_table, right_table, join_key)`.
    pub join_path: Vec<(String, String, String)>,
}

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        use airlayer::engine::query::{
            FilterOperator, OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery,
        };

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

/// Lightweight hypothesis produced by the Triage sub-phase of Clarify.
///
/// Triage runs *without* tools or column-level schema — it only sees table
/// names.  Its job is to narrow the search space before the heavier Ground
/// sub-phase explores columns via tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainHypothesis {
    /// Natural-language summary of what the user is asking about.
    /// e.g. "The user wants to see their weight trend over recent weeks."
    pub summary: String,
    /// Table names likely relevant to the question (subset of all tables).
    pub relevant_tables: Vec<String>,
    /// Broad question category chosen *before* seeing column details.
    pub question_type: QuestionType,
    /// Inferred time scope, if any (e.g. "last 30 days", "this year").
    #[serde(default)]
    pub time_scope: Option<String>,
    /// How confident the model is in its interpretation (0.0–1.0).
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Language-level ambiguities in the user's question that cannot be
    /// resolved without asking the user — e.g. "unclear which metric 'progress'
    /// refers to" or "time range is unspecified".  Empty when the question is
    /// unambiguous.
    #[serde(default)]
    pub ambiguities: Vec<String>,
    /// Structured version of `ambiguities` — each entry is a question with
    /// LLM-generated suggestions.  Populated when the triage schema includes
    /// `ambiguity_questions`; falls back to constructing from `ambiguities`
    /// with empty suggestions when absent.
    #[serde(default)]
    pub ambiguity_questions: Vec<HumanInputQuestion>,
}

fn default_confidence() -> f32 {
    1.0
}

/// The user-facing analytics request produced by the Clarify stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsIntent {
    /// Original natural-language question.
    pub raw_question: String,
    /// Classified question type.
    pub question_type: QuestionType,
    /// Metric names the user cares about (e.g. `["revenue", "orders"]`).
    pub metrics: Vec<String>,
    /// Grouping dimensions (e.g. `["region", "product_category"]`).
    pub dimensions: Vec<String>,
    /// Filter expressions in a simple DSL (e.g. `["date >= '2024-01-01'"]`).
    pub filters: Vec<String>,
    /// Prior question–answer turns from the current conversation session.
    ///
    /// Empty for the first question.  Populated by the caller before passing
    /// a follow-up question into [`Orchestrator::run`].  The Clarify and
    /// Interpret prompt builders inject this history so the LLM can resolve
    /// references like "compare that to last year" or maintain a consistent
    /// tone.
    #[serde(default)]
    pub history: Vec<ConversationTurn>,
    /// Confirmed schema resolutions from a prior Specifying attempt, if any.
    ///
    /// Set on back-edges where a `QuerySpec` was produced but failed downstream
    /// (validation or execution).  Injected into the retry Specify prompt so
    /// the LLM reuses known-good resolutions rather than re-deriving them.
    #[serde(default)]
    pub spec_hint: Option<SpecHint>,
    /// Procedure file selected by the LLM during the Ground sub-phase.
    ///
    /// When the LLM calls `search_procedures` and finds a file that directly
    /// answers the question, it sets this path in its structured response.
    /// The Specifying stage short-circuits: it skips LLM resolution and
    /// emits a `QuerySpec` with `SolutionSource::Procedure { file_path }`
    /// so execution jumps straight to the Executing stage.
    #[serde(default)]
    pub selected_procedure: Option<std::path::PathBuf>,
}

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
// Error
// ---------------------------------------------------------------------------

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
}

impl std::fmt::Display for AnalyticsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyticsError::UnresolvedMetric { metric } => {
                write!(f, "unresolved metric or column: '{metric}' — check the schema and use a fully-qualified table.column reference")
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
        }
    }
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
}

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
