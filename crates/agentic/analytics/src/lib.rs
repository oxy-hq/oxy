//! Analytics domain implementation.
//!
//! Implements [`Domain`] for structured data analytics queries — turning
//! natural-language questions into resolved query specs that can be executed
//! against a columnar data store.
//!
//! # Catalog implementations
//!
//! | Type | Description |
//! |---|---|
//! | [`SchemaCatalog`] | Raw database schema; `try_compile` always returns `TooComplex` |
//! | [`SemanticCatalog`] | Oxy semantic layer; compiles simple group-by queries directly |
//! | [`SemanticCatalog`] | Semantic + schema fallback; primary runtime type |
//!
//! # Validation functions
//!
//! Three pure functions are provided, one per pipeline stage:
//!
//! | Function | Stage | Checks |
//! |---|---|---|
//! | [`validate_specified`] | `specify` | metrics/dimensions/joins/filters resolve to real columns |
//! | [`validate_solvable`] | `solve` | SQL is syntactically sound and references known tables |
//! | [`validate_solved`] | `execute` | results non-empty, shape matches spec, values plausible |

pub mod airlayer_compat;
mod catalog;
pub mod config;
pub mod context_budget;
pub mod engine;
mod events;
mod llm;
pub mod procedure;
mod schemas;
mod semantic;
mod solver;
pub mod tools;
mod types;
mod ui;
mod validation;

// ── Engine ────────────────────────────────────────────────────────────────────

pub use engine::{EngineError, SemanticEngine, TranslationContext, VendorQuery};

// ── Catalog ───────────────────────────────────────────────────────────────────

pub use catalog::{
    Catalog, CatalogError, CatalogSearchResult, ColumnRange, DimensionSummary, JoinPath, MetricDef,
    MetricSummary, QueryContext, SchemaCatalog, SemanticLayerError,
};
pub use semantic::SemanticCatalog;

// ── Events / LLM ─────────────────────────────────────────────────────────────

pub use events::{AnalyticsEvent, ProcedureStepInfo};
pub use llm::{
    AnthropicProvider, Chunk, ContentBlock, DEFAULT_MODEL, LlmClient, LlmError, LlmOutput,
    LlmProvider, OpenAiProvider, ReasoningEffort, ResponseSchema, ThinkingConfig, ToolCallChunk,
    ToolLoopConfig, Usage as LlmUsage,
};
pub use schemas::{
    clarify_response_schema, solve_response_schema, specify_response_schema,
    specify_response_schema_legacy, triage_response_schema,
};

// ── Config ────────────────────────────────────────────────────────────────────

pub use config::{AgentConfig, BuildContext, ConfigError, StateConfig, ThinkingConfigYaml};

// ── Procedure ─────────────────────────────────────────────────────────────────

pub use procedure::{ProcedureError, ProcedureOutput, ProcedureRef, ProcedureRunner};

// ── Solver ────────────────────────────────────────────────────────────────────

pub use solver::{AnalyticsSolver, build_analytics_handlers};

// ── Tools ─────────────────────────────────────────────────────────────────────

pub use tools::{
    SchemaCache, clarifying_tools, execute_clarifying_tool, execute_database_lookup_tool,
    execute_interpreting_tool, execute_solving_tool, execute_specifying_tool, interpreting_tools,
    new_schema_cache, solving_tools, specifying_tools,
};

// ── Domain types ──────────────────────────────────────────────────────────────

pub use types::{
    AnalyticsAnswer, AnalyticsCatalog, AnalyticsDomain, AnalyticsError, AnalyticsIntent,
    AnalyticsResult, AnalyticsSolution, ChartConfig, ConversationTurn, DomainHypothesis,
    QueryRequestItem, QueryResultSet, QuerySpec, QuestionType, ResultShape, SolutionPayload,
    SolutionSource, SpecHint,
};

// ── Validation ────────────────────────────────────────────────────────────────

pub use validation::{
    RegistryError, ValidationConfig, Validator, validate_solvable, validate_solved,
    validate_specified,
};

// ── UI ────────────────────────────────────────────────────────────────────────

pub use ui::{analytics_step_summary, analytics_tool_summary};
