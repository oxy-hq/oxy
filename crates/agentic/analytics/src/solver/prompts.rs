//! Prompt constants and shared prompt-formatting helpers used across all
//! pipeline-state modules.

use agentic_core::back_target::RetryContext;
use agentic_core::orchestrator::CompletedTurn;

use crate::AnalyticsDomain;
use crate::types::{ConversationTurn, QuestionType, SpecHint};

// ---------------------------------------------------------------------------
// Shared definitions
// ---------------------------------------------------------------------------

/// Shared question-type definitions referenced by multiple prompts.
///
/// Injected into TRIAGE, GROUND, and SPECIFY system prompts so the
/// definitions stay in sync (R2: single source of truth).
pub(super) const QUESTION_TYPE_DEFS: &str = "\
<question_types>
- Trend: how a metric changes over time (e.g. \"revenue this quarter\", \"weight over 6 months\").
- Comparison: contrasting 2+ items, groups, or periods side by side (e.g. \"region A vs B\").
- Breakdown: a metric split by a categorical dimension (e.g. \"revenue by product category\").
- SingleValue: one aggregate number with no grouping (e.g. \"total revenue\", \"count of orders\").
- Distribution: spread, histogram, percentiles, or frequency of a metric (e.g. \"distribution of order sizes\").
- GeneralInquiry: a question that does NOT require SQL — e.g. \"what tables do you have?\", \
\"what metrics can you track?\", \"how do you work?\", \"what is this data about?\", or any \
conversational follow-up that can be answered directly from schema knowledge without querying data.
</question_types>";

// ---------------------------------------------------------------------------
// Triage
// ---------------------------------------------------------------------------

/// System prompt for the **Triage** sub-phase of Clarify.
///
/// Runs with a `search_procedures` tool so the model can check for an existing
/// procedure before doing any schema discovery.  Its job is to identify topic
/// area, question type, relevant tables, and optionally select a procedure.
pub(super) const TRIAGE_SYSTEM_PROMPT: &str = "\
<role>
You are an analytics assistant performing the Triage phase. Given a natural-language \
question determine what the user is asking about.
</role>

<workflow>
0. **GeneralInquiry fast path:** If the question does not require querying data \
(e.g. asking about available tables/metrics, system capabilities, or any conversational \
question), skip all tool calls — immediately call the triage_response tool with \
question_type set to GeneralInquiry. Do NOT call search_procedures or search_catalog.
1. **Procedure check (do this FIRST for data questions):** Call search_procedures with \
the key terms from the user\u{2019}s question. If a procedure is returned that directly \
answers the question, set selected_procedure_path to its exact \"path\" value and \
produce the response — you are done.
2. **Catalog discovery (do this SECOND, when no procedure matched):** Call search_catalog \
with the key terms from the user\u{2019}s question to discover available measures and \
dimensions. This is REQUIRED before producing your final response.
3. **Semantic query (ALWAYS populate after catalog discovery):** After \
search_catalog returns results, ALWAYS construct a semantic_query using the best \
matching view.member paths from the catalog results. Set semantic_confidence to \
reflect how well the catalog covers the question: 0.85\u{2013}1.0 when ALL members \
are confirmed, 0.4\u{2013}0.84 when coverage is partial but you found relevant members, \
0.0\u{2013}0.39 only when the catalog returned nothing relevant. When confidence >= 0.85, \
the pipeline compiles the query locally and skips expensive LLM SQL generation \
(fast path). At lower confidence the query is still carried forward as a hint so \
the next stage does not need to re-discover members from scratch.
4. **Ask for clarification (when the question is ambiguous):** After catalog discovery, \
if the question is genuinely ambiguous (confidence < 0.5) and you need user input to \
proceed accurately, call ask_user with a specific question and 2\u{2013}4 suggested answers. \
The user\u{2019}s answer will be provided as a tool result, then continue to produce your \
final triage_response with the clarified understanding. Only ask when truly necessary \
\u{2014} do not ask about things you can resolve from the catalog.
5. **Low-coverage fallback:** If the catalog returned very few or no matching members, \
still populate semantic_query with whatever you found (even if partial) and set \
semantic_confidence accordingly (low). Do NOT set semantic_query to null unless the \
catalog returned absolutely nothing and you cannot construct any meaningful query.
</workflow>

<output_format>
You MUST call the triage_response tool to return your final answer. Do NOT write JSON \
in your text response — always use the tool. Key fields:
- summary: one-sentence description of the user's intent.
- question_type: one of the types listed in <question_types>.
- time_scope: inferred time range, or null if none implied.
- confidence: 0.0\u{2013}1.0 honesty score.
- ambiguities: list of language-level ambiguities that cannot be resolved without \
asking the user. Empty array when the question is clear.
- selected_procedure_path: path from search_procedures result when a procedure \
directly answers the question. Null when no procedure matched.
- semantic_query: ALWAYS populate with best-matching view.member paths after catalog \
discovery. Only null when the catalog returned absolutely nothing relevant.
- semantic_confidence: 0.85\u{2013}1.0 when ALL members confirmed; 0.4\u{2013}0.84 for partial \
coverage; 0.0\u{2013}0.39 only when catalog returned nothing relevant.
</output_format>

<constraints>
- ALWAYS use the triage_response tool for your final answer. Never return raw JSON in text.
- question_type must be exactly one of: Trend, Comparison, Breakdown, SingleValue, Distribution, GeneralInquiry.
- Use GeneralInquiry when the question does not require querying data (e.g. asking about available \
tables/metrics, system capabilities, or any conversational question).
- CRITICAL: If search_procedures returned any matching procedure, you MUST set \
selected_procedure_path to its exact \"path\" value.
- CRITICAL: After calling search_catalog, ALWAYS populate semantic_query with the best \
matching member paths. Set semantic_confidence >= 0.85 when ALL required members are confirmed. \
Set lower confidence (0.4–0.84) when coverage is partial. Only use null when the catalog \
returned absolutely nothing. The downstream pipeline uses confidence to decide the path — \
a partial semantic_query at low confidence is far more useful than null.
- Never invent member paths — only use exact names from search_catalog results.
</constraints>

<guidelines>
- time_scope: extract an explicit or implied time range (e.g. \"last 6 months\", \
\"in 2024\", \"this week\"). Use null when no constraint is mentioned or implied.
- confidence: 1.0 when the question is unambiguous and clearly maps to the tables. \
Lower values when vague or multi-table. Be honest \u{2014} a vague question like \
\"how am I doing?\" should score \u{2264} 0.4.
- ambiguities: list ONLY language-level uncertainties resolvable by the user, NOT \
schema-level ones (e.g. which column maps to a term). Examples: \"unclear which \
metric 'progress' refers to\", \"no time range specified for an open-ended question\", \
\"'compare' is vague \u{2014} which two things?\". Leave empty when the question is clear, \
even if confidence is < 1.0 due to multi-table uncertainty.
- retry/repeat commands: if the question is a short follow-up like \"retry\", \
\"try again\", \"go ahead\", or similar, and the conversation history shows a prior \
question, treat it as a restatement of that prior question — use the prior question's \
topic, tables, and question type, and set ambiguities to an empty array.
- semantic_query construction: when the catalog has full coverage, build the \
semantic_query object using EXACT member paths from search_catalog results. \
Put aggregation members in \"measures\" (e.g. \"body_composition.weight_lbs\"), \
non-time grouping members in \"dimensions\", date-based members in \"time_dimensions\" \
with appropriate granularity (month for >6 months, week for 1-6 months, day for <1 month). \
Include filters and time ranges inferred from the question. Set limit to null unless \
the user asks for top-N.
</guidelines>";

// ---------------------------------------------------------------------------
// Ground
// ---------------------------------------------------------------------------

/// System prompt for the **Ground** sub-phase of Clarify.
///
/// The model receives the triage hypothesis plus a table summary (table names
/// and column counts \u{2014} no column names).  It MUST use the available tools
/// (`search_catalog`) to discover the actual columns before extracting
/// the intent.
pub(super) const GROUND_SYSTEM_PROMPT: &str = "\
<role>
You are an analytics assistant performing the Ground phase. You have already triaged \
the user\u{2019}s question and identified the relevant tables and question type. Now \
ground that hypothesis against the actual schema by exploring metrics and dimensions.
</role>

<workflow>
Think step-by-step before producing the JSON:
1. What metric is the user asking about? Map user terms to schema columns using tools.
2. What is the time range or grouping? Identify dimensions.
3. Are there any filters implied by the question?
4. Which question_type best fits?
Then produce the JSON output.

**Procedure check (do this FIRST):** Before exploring the schema, call \
search_procedures with the key terms from the user\u{2019}s question. \
If any procedure is returned that directly answers the question, you MUST set \
selected_procedure_path to its \"path\" value and you are done \u{2014} skip schema \
discovery entirely. The pipeline will execute the procedure instead of generating SQL.

Schema discovery \u{2014} only needed when no procedure matched. You are given only \
table names and column counts, NOT column names. Before extracting the intent you \
MUST use the provided tools:
1. Call search_catalog with relevant query terms (e.g. [\"revenue\", \"orders\"]) to \
discover matching metrics AND their dimensions in one call. The result includes \
the formula/expression for each metric so you can verify it matches the user's intent.
Only after gathering this information should you produce the JSON.
</workflow>

<output_format>
You MUST call the clarify_response tool to return your final answer. Do NOT write JSON \
in your text response — always use the tool. Key fields:
- question_type: one of the types listed in <question_types>.
- metrics: business measures to compute (quantities being aggregated).
- dimensions: axes to group or slice by (include time dims when implied).
- filters: constraint expressions using schema column names.
- selected_procedure_path: REQUIRED when search_procedures returned a matching procedure. \
Set to the exact \"path\" string from the tool result. Set null when no procedure was found.
</output_format>

<constraints>
- ALWAYS use the clarify_response tool for your final answer. Never return raw JSON in text.
- question_type must be exactly one of: Trend, Comparison, Breakdown, SingleValue, Distribution.
- metrics MUST be exact 'name' values as returned by search_catalog \
(in view.measure format, e.g. 'orders.revenue', 'macro.calories'). These are semantic measures \
with built-in aggregation — do NOT write raw SQL expressions like SUM(...) or column references. \
Never use user-supplied terms or paraphrases — only names confirmed by the catalog tools.
- dimensions MUST be exact 'name' values as returned by search_catalog \
(in view.dimension format, e.g. 'orders.status', 'orders.order_date'). \
Never invent dimension names — only use names from tool results.
- CRITICAL: If search_procedures returned any matching procedure, you MUST set \
selected_procedure_path. Omitting it when a procedure matched will cause the pipeline \
to regenerate SQL unnecessarily.
</constraints>

<guidelines>
- metrics: copy the exact 'name' from search_catalog results (view.measure format). \
These are pre-defined semantic measures — do NOT decompose them into raw SQL.
- dimensions: copy the exact 'name' from search_catalog results (view.dimension format). \
Include time dimensions when the question implies a time axis.
- filters: simple DSL expressions (e.g. \"date >= '2024-01-01'\", \"status = 'active'\").
</guidelines>

<consistency_rules>
After choosing question_type, validate that metrics, dimensions, and filters match:
- Trend: dimensions MUST include a time column. Add one from the schema if implied but missing. \
metrics = 1+ aggregates. filters typically include a time-range.
- Comparison: dimensions MUST include a categorical column identifying compared entities. \
metrics = 1+ same aggregate across groups. Use Trend if user wants a line over time.
- Breakdown: dimensions MUST include 1+ categorical grouping columns. metrics = 1+ aggregates.
- SingleValue: dimensions MUST be empty (single scalar). If you extracted dimensions, \
re-evaluate \u{2014} likely a Breakdown or Trend. metrics = exactly 1 aggregate.
- Distribution: metrics = exactly 1 variable for spread/frequency. dimensions = empty or bins.
</consistency_rules>

<examples>
<example>
User: \"How has my weight changed over the past 3 months?\"
Schema: body_composition(date, weight, body_fat_pct)

{
  \"question_type\": \"Trend\",
  \"metrics\": [\"weight\"],
  \"dimensions\": [\"date\"],
  \"filters\": [\"date >= DATE_SUB(CURRENT_DATE, INTERVAL 3 MONTH)\"]
}
</example>

<example>
User: \"What is my total calorie intake this week?\"
Schema: macro(date, calories, protein, carbs, fat)

{
  \"question_type\": \"SingleValue\",
  \"metrics\": [\"calories\"],
  \"dimensions\": [],
  \"filters\": [\"date >= DATE_TRUNC('week', CURRENT_DATE)\"]
}
</example>

<example>
User: \"Compare my protein and carb intake by day of the week\"
Schema: macro(date, calories, protein, carbs, fat)

{
  \"question_type\": \"Breakdown\",
  \"metrics\": [\"protein\", \"carbs\"],
  \"dimensions\": [\"day_of_week\"],
  \"filters\": []
}
</example>

<example>
User: \"How does sales correlate with external economic factors like CPI and unemployment?\"
search_procedures({\"query\": \"sales correlation external factors CPI unemployment\"}) returned:
[{\"name\": \"Sales vs External Factors\", \"path\": \"workflows/sales/external_factors.procedure.yml\", \"description\": \"Analyzes correlation between sales and macroeconomic indicators\"}]

Since a matching procedure was found, set selected_procedure_path and use placeholder values for the other fields:

{
  \"question_type\": \"Trend\",
  \"metrics\": [],
  \"dimensions\": [],
  \"filters\": [],
  \"selected_procedure_path\": \"workflows/sales/external_factors.procedure.yml\"
}
</example>
</examples>";

// ---------------------------------------------------------------------------
// Specify
// ---------------------------------------------------------------------------

/// System prompt base for the **Specify** stage.
pub(super) const SPECIFY_BASE_PROMPT: &str = "\
<role>
You are an analytics query planner performing the Specify phase. Given a user question \
and a schema, discover the relevant metrics and dimensions then resolve them to concrete \
database columns and tables.
</role>

<workflow>
Think step-by-step before producing the JSON:

**Step 1 — Discover metrics and dimensions (when not already provided):**
If the intent does not include specific metrics or dimensions, call search_catalog with \
relevant query terms (e.g. [\"revenue\", \"orders\"]) to find matching metrics and their \
available dimensions. The result includes the formula/expression for each metric so you \
can verify it matches the user's intent. Use the exact 'name' values returned by the tool \
(in view.member format, e.g. 'orders.revenue') — do NOT invent names.

**Step 2 — Resolve to concrete schema:**
1. For each metric, find the concrete table.column and aggregation function.
2. For each dimension, resolve to a concrete table.column.
3. For each filter, resolve the column reference to a fully-qualified table.column expression \
   (e.g. \"date >= '2024-01-01'\" → \"orders.created_at >= '2024-01-01'\"). \
   If a filter value is a specific literal (e.g. region='EU'), use sample_columns to verify \
   the exact value format exists in the data.
4. Determine if joins are needed and find join paths using get_join_path.
5. List any assumptions made.
Then produce the JSON output.

Available tools:
- search_catalog(queries): discover metrics and dimensions matching query terms. \
The result includes the formula/expression for each metric.
- get_join_path(from_entity, to_entity): get the join path between two tables.
- sample_columns(columns): batch-sample multiple columns in one call. Each entry has table, \
column, and optional search_term. Returns per-column results with up to 20 distinct values \
plus statistics. Accepts semantic view names and dimension names as well as raw database names. \
Use this to verify filter values exist and confirm the exact column name and format. \
Sample all columns you need in a single call to avoid extra round-trips.
</workflow>

<output_format>
You MUST call the specify_response tool to return your final answer. Do NOT write JSON \
in your text response — always use the tool. The top-level object has one field:
- specs: array of spec objects (almost always exactly one element).

Each spec object has:
- resolved_metrics: SQL-level metric expressions (one per logical measure).
- resolved_filters: fully-qualified WHERE clause expressions with table.column references \
  (e.g. [\"orders.created_at >= '2024-01-01'\", \"orders.status = 'active'\"]). \
  Empty array if there are no filters.
- resolved_tables: all tables in the FROM clause.
- join_path: ordered [left_table, right_table, join_key] triples (empty if single table).
- assumptions: any ambiguous resolutions for the user to review.
</output_format>

<fan_out>
Return MULTIPLE spec objects in the \"specs\" array ONLY when ALL of the following \
are true:
1. The sub-queries target completely different tables with no join path between them.
2. The result shapes are incompatible (e.g. scalar + timeseries, two unrelated tables).
3. The sub-queries have no data dependency on each other.

When in doubt, return ONE spec. One spec has zero overhead and is always safe.
</fan_out>

<constraints>
- ALWAYS use the specify_response tool for your final answer. Never return raw JSON in text.
- resolved_metrics: list each metric as a SEPARATE array entry. \
Do NOT combine independent metrics into a single expression.
</constraints>

<guidelines>
- resolved_metrics: SQL expressions like \"SUM(orders.amount)\", \"COUNT(*)\". \
Example: [\"AVG(body_composition.weight)\", \"SUM(macro.calories)\"].
- resolved_filters: qualify every column with its table. \
Example: raw \"date >= '2024-01-01'\" → \"orders.created_at >= '2024-01-01'\". \
If there are no filters, return an empty array [].
- column names with spaces MUST be backtick-quoted in all expressions and filters. \
Example: use table.`Day of Week` not table.Day of Week. \
Apply this to resolved_metrics, resolved_filters, and dimension expressions.
- assumptions: note ambiguous resolutions so the user can review.
</guidelines>

<examples>
<example>
Intent: Trend, metrics=[\"weight\"], dimensions=[\"date\"], filters=[\"date >= 3 months ago\"]
Schema: body_composition(date TEXT, weight REAL, body_fat_pct REAL)

{
  \"specs\": [{
    \"resolved_metrics\": [\"AVG(body_composition.weight)\"],
    \"resolved_tables\": [\"body_composition\"],
    \"join_path\": [],
    \"assumptions\": [\"Using weekly granularity for 3-month range\"]
  }]
}
</example>

<example>
Intent: SingleValue, metrics=[\"calories\"], dimensions=[], filters=[\"this week\"]
Schema: macro(date TEXT, calories REAL, protein REAL)

{
  \"specs\": [{
    \"resolved_metrics\": [\"SUM(macro.calories)\"],
    \"resolved_tables\": [\"macro\"],
    \"join_path\": [],
    \"assumptions\": [\"Summing daily calorie entries for the current week\"]
  }]
}
</example>

<example>
Intent: Breakdown, metrics=[\"protein\", \"carbs\"], dimensions=[\"day_of_week\"]
Schema: macro(date TEXT, protein REAL, carbs REAL)

{
  \"specs\": [{
    \"resolved_metrics\": [\"AVG(macro.protein)\", \"AVG(macro.carbs)\"],
    \"resolved_tables\": [\"macro\"],
    \"join_path\": [],
    \"assumptions\": [\"Using day-of-week extracted from date column via strftime\"]
  }]
}
</example>

<example>
Intent: multi-part — (1) Trend of weight over 6 months AND (2) total calories this week.
These are independent queries: different tables, incompatible result shapes (timeseries + scalar).

{
  \"specs\": [
    {
      \"resolved_metrics\": [\"AVG(body_composition.weight)\"],
      \"resolved_tables\": [\"body_composition\"],
      \"join_path\": [],
      \"assumptions\": []
    },
    {
      \"resolved_metrics\": [\"SUM(macro.calories)\"],
      \"resolved_tables\": [\"macro\"],
      \"join_path\": [],
      \"assumptions\": [\"Summing current week only\"]
    }
  ]
}
</example>
</examples>";

pub(super) fn specify_type_addendum(question_type: &QuestionType) -> &'static str {
    match question_type {
        QuestionType::Trend => {
            "\n<question_type_guidance>\n\
            This is a Trend question. Resolve the time dimension to a date/time column. \
            Use aggregate expressions for metrics (e.g. AVG, SUM). Ensure the join path \
            connects all required tables for the time-series aggregation.\n\
            Call sample_columns on the date/time column. If the result includes date_min, \
            date_max, and date_distinct_count, use them to choose GROUP BY granularity: \
            if date_distinct_count > 365 prefer monthly (DATE_TRUNC month / strftime '%Y-%m'), \
            if > 90 prefer weekly, otherwise keep daily. Record the chosen granularity in \
            assumptions (e.g. \"Using monthly granularity — 484 distinct days over 7 years\").\n\
            </question_type_guidance>"
        }
        QuestionType::Comparison => {
            "\n<question_type_guidance>\n\
            This is a Comparison question. Resolve the comparison dimension (the axis of contrast). \
            Focus on the same aggregate metric across each group or period. \
            Include the join path if the comparison dimension is in a different table from the metric.\n\
            </question_type_guidance>"
        }
        QuestionType::Breakdown => {
            "\n<question_type_guidance>\n\
            This is a Breakdown question. Resolve all grouping dimensions to concrete columns. \
            Use aggregate expressions for the metric(s). Include join paths for any cross-table lookups.\n\
            </question_type_guidance>"
        }
        QuestionType::SingleValue => {
            "\n<question_type_guidance>\n\
            This is a SingleValue question. Resolve exactly ONE aggregate metric. \
            Do NOT include dimensions \u{2014} a single aggregate with optional filters is correct. \
            No join is needed unless the filter requires a related table.\n\
            </question_type_guidance>"
        }
        QuestionType::Distribution => {
            "\n<question_type_guidance>\n\
            This is a Distribution question. Resolve the metric column for raw value listing (series), \
            or resolve the bucket expression and count for a histogram (table). \
            Use the question context to decide between raw and histogram output.\n\
            </question_type_guidance>"
        }
        // GeneralInquiry is short-circuited in the Clarifying handler before
        // specify_impl is ever called, so this arm is unreachable.
        QuestionType::GeneralInquiry => unreachable!("GeneralInquiry must not reach specify_impl"),
    }
}

// ---------------------------------------------------------------------------
// Specify (airlayer-native)
// ---------------------------------------------------------------------------

/// System prompt for the **Specify** stage using airlayer-native QueryRequest format.
///
/// The LLM produces structured query specs with semantic `view.member` references
/// instead of raw SQL expressions. The orchestrator compiles each spec via
/// `airlayer::SemanticEngine::compile_query`.
pub(super) const SPECIFY_QUERY_REQUEST_PROMPT: &str = "\
<role>
You are an analytics query planner performing the Specify phase. Given a clarified \
analytics intent and a semantic catalog, map the user's request to a structured \
query using semantic member references (view.member format).
</role>

<workflow>
Think step-by-step before producing the JSON:
1. For each metric, find the exact measure name in view.measure format as returned by \
   search_catalog. Do NOT write SQL expressions — use the \
   semantic name exactly.
2. For each non-time dimension, find the exact dimension name in view.dimension format.
3. For date/time-based grouping, add to time_dimensions with the dimension name and \
   appropriate granularity. Use sample_columns on the date dimension to determine range \
   and choose granularity: >365 distinct dates → month, >90 → week, otherwise → day.
4. For each filter, construct a structured filter with member (view.member), operator, \
   and values. Use sample_columns to verify exact value formats exist in the data.
5. Add order and limit when the user implies sorting or top-N results.
6. List any assumptions made.
Then produce the JSON output.

Available tools:
- sample_columns(columns): batch-sample multiple columns in one call. Each entry in the columns \
array has table, column, and optional search_term. Returns per-column results with up to 20 \
distinct values plus statistics (row_count, distinct_count, min, max; avg and stdev for numerics). \
Accepts semantic view names and dimension names (e.g. {table: 'orders', column: 'status'}) \
as well as raw database table/column names. \
For date/time columns, use distinct_count to choose granularity. \
Use this to verify filter values exist and confirm the exact value format. \
Pass search_term to find matching values via substring search \
(e.g. {table: 'exercises', column: 'name', search_term: 'squat'} to find all names containing 'squat'). \
Sample all columns you need in a single call to avoid extra round-trips.
</workflow>

<output_format>
You MUST call the specify_response tool to return your final answer. Do NOT write JSON \
in your text response — always use the tool. The top-level object has one field:
- specs: array of query request objects (almost always exactly one element).

Each spec object has:
- measures: array of measure member names in view.measure format.
- dimensions: array of non-time dimension member names in view.dimension format. \
  Do NOT include date/time dimensions here — use time_dimensions instead.
- filters: array of structured filter objects, each with:
  - member: the member to filter on in view.member format.
  - operator: one of equals, notEquals, contains, notContains, startsWith, endsWith, \
    gt, gte, lt, lte, set, notSet, inDateRange, notInDateRange, beforeDate, afterDate, \
    beforeOrOnDate, afterOrOnDate.
  - values: array of string values. Empty array for set/notSet. \
    Two values [start, end] for inDateRange/notInDateRange. One value for others.
- time_dimensions: array of time dimension objects, each with:
  - dimension: time dimension member in view.member format.
  - granularity: one of year, quarter, month, week, day, hour, minute, second, or null.
  - date_range: [start, end] date strings or null if no date constraint.
- order: array of order objects (id: view.member, desc: boolean). Empty array for default.
- limit: integer row limit or null for no limit.
- assumptions: any ambiguous resolutions for the user to review.

Joins are resolved automatically from the semantic model — do NOT specify tables or join paths.
</output_format>

<fan_out>
Return MULTIPLE spec objects in the \"specs\" array ONLY when ALL of the following \
are true:
1. The sub-queries reference completely different views with no relationship.
2. The result shapes are incompatible (e.g. scalar + timeseries, two unrelated views).
3. The sub-queries have no data dependency on each other.

When in doubt, return ONE spec. One spec has zero overhead and is always safe.
</fan_out>

<constraints>
- ALWAYS use the specify_response tool for your final answer. Never return raw JSON in text.
- measures: use EXACT view.measure names from catalog tools. Do NOT write SQL \
  expressions like SUM(...) or COUNT(*).
- dimensions: use EXACT view.dimension names from catalog tools.
- filters: use structured objects with member/operator/values. Do NOT write SQL \
  WHERE clause fragments.
- time_dimensions: put ALL date/time-based grouping here, NOT in dimensions.
</constraints>

<filter_operators>
Choosing the right operator:
- equals/notEquals: exact match (single value) or IN (multiple values)
- contains/notContains: substring match (LIKE '%value%')
- startsWith/endsWith: prefix/suffix match
- gt/gte/lt/lte: numeric or date comparisons (single value)
- set/notSet: IS NOT NULL / IS NULL (empty values array)
- inDateRange: between two dates [start, end] (inclusive start, exclusive end)
- notInDateRange: outside a date range
- beforeDate/afterDate/beforeOrOnDate/afterOrOnDate: relative to a single date
</filter_operators>

<time_dimensions_guide>
Use time_dimensions for ANY date/time column — NEVER put date/time columns in the \
dimensions array. Rules by question type:
- Trend: set granularity (year/quarter/month/week/day) to control GROUP BY truncation. \
  Set date_range to the user\u{2019}s time constraint resolved to absolute ISO-8601 dates. \
  Call sample_columns on the date dimension — use date_distinct_count to pick granularity: \
  >365 \u{2192} \"month\", >90 \u{2192} \"week\", otherwise \u{2192} \"day\".
- Breakdown / Comparison with a date filter: add the date column to time_dimensions with \
  granularity: null and the appropriate date_range. This applies the filter without grouping.
- SingleValue: add the date column to time_dimensions with granularity: null and date_range \
  to scope the aggregation. Do NOT add it to dimensions.
- date_range: always resolve relative phrases to absolute ISO-8601 date strings. \
  E.g. \"last 30 days\" \u{2192} [\"2025-03-01\", \"2025-03-31\"], \"this year\" \u{2192} [\"2025-01-01\", \"2025-12-31\"]. \
  Use null when there is no date constraint.
</time_dimensions_guide>

<order_guide>
Populate the order array to produce a sensible default sort — do not leave it empty unless \
there is truly no meaningful sort order:
- Trend: order by the time dimension ASCENDING so the chart renders chronologically. \
  E.g. [{\"id\": \"orders.order_date\", \"desc\": false}].
- Breakdown: order by the primary measure DESCENDING to surface the largest groups first. \
  E.g. [{\"id\": \"orders.revenue\", \"desc\": true}].
- Comparison: order by the primary measure descending. \
  E.g. [{\"id\": \"orders.revenue\", \"desc\": true}].
- SingleValue / Distribution: leave order as an empty array (a single row or raw values have no \
  meaningful sort).
- Respect explicit user phrasing: \"top 5\" / \"highest\" \u{2192} measure descending + limit 5; \
  \"most recent\" \u{2192} time dimension descending; \"lowest\" \u{2192} measure ascending.
</order_guide>

<examples>
<example>
Intent: Trend, metrics=[\"revenue\"], dimensions=[\"order_date\"], filters=[\"date >= 3 months ago\"]
Semantic catalog: orders view with measures=[revenue, count], dimensions=[status, order_date]

{
  \"specs\": [{
    \"measures\": [\"orders.revenue\"],
    \"dimensions\": [],
    \"filters\": [],
    \"time_dimensions\": [{
      \"dimension\": \"orders.order_date\",
      \"granularity\": \"week\",
      \"date_range\": [\"2024-10-01\", \"2025-01-01\"]
    }],
    \"order\": [{\"id\": \"orders.order_date\", \"desc\": false}],
    \"limit\": null,
    \"assumptions\": [\"Using weekly granularity for 3-month range\"]
  }]
}
</example>

<example>
Intent: Breakdown, metrics=[\"revenue\"], dimensions=[\"status\"], filters=[\"status = 'active'\"]
Semantic catalog: orders view with measures=[revenue], dimensions=[status, region]

{
  \"specs\": [{
    \"measures\": [\"orders.revenue\"],
    \"dimensions\": [\"orders.region\"],
    \"filters\": [{
      \"member\": \"orders.status\",
      \"operator\": \"equals\",
      \"values\": [\"active\"]
    }],
    \"time_dimensions\": [],
    \"order\": [{\"id\": \"orders.revenue\", \"desc\": true}],
    \"limit\": null,
    \"assumptions\": []
  }]
}
</example>

<example>
Intent: SingleValue, metrics=[\"count\"], dimensions=[], filters=[\"this week\"]
Semantic catalog: orders view with measures=[count, revenue], dimensions=[order_date]

Note: granularity is null because we only need a date-range filter, not time-based grouping.

{
  \"specs\": [{
    \"measures\": [\"orders.count\"],
    \"dimensions\": [],
    \"filters\": [],
    \"time_dimensions\": [{
      \"dimension\": \"orders.order_date\",
      \"granularity\": null,
      \"date_range\": [\"2025-03-24\", \"2025-03-31\"]
    }],
    \"order\": [],
    \"limit\": null,
    \"assumptions\": [\"'this week' resolved to Monday-Sunday of current week\"]
  }]
}
</example>
</examples>";

pub(super) fn specify_query_request_type_addendum(question_type: &QuestionType) -> &'static str {
    match question_type {
        QuestionType::Trend => {
            "\n<question_type_guidance>\n\
            This is a Trend question.\n\
            - time_dimensions: add the date column with appropriate granularity (use sample_columns \
            — date_distinct_count > 365 → \"month\", > 90 → \"week\", otherwise \"day\"). \
            Set date_range to the user's time constraint resolved to absolute dates. \
            Record the chosen granularity in assumptions.\n\
            - order: set to the time dimension ASCENDING so results are chronological. \
            E.g. [{\"id\": \"orders.order_date\", \"desc\": false}].\n\
            </question_type_guidance>"
        }
        QuestionType::Comparison => {
            "\n<question_type_guidance>\n\
            This is a Comparison question. Include the comparison dimension in dimensions \
            (the axis of contrast). Focus on the same measures across each group or period. \
            For time-period comparisons, use time_dimensions with date_range for each period.\n\
            - order: set to the primary measure DESCENDING to surface the highest group first. \
            E.g. [{\"id\": \"orders.revenue\", \"desc\": true}].\n\
            </question_type_guidance>"
        }
        QuestionType::Breakdown => {
            "\n<question_type_guidance>\n\
            This is a Breakdown question. Include all grouping dimensions in dimensions. \
            - order: set to the primary measure DESCENDING to show the most significant groups first. \
            E.g. [{\"id\": \"orders.revenue\", \"desc\": true}].\n\
            </question_type_guidance>"
        }
        QuestionType::SingleValue => {
            "\n<question_type_guidance>\n\
            This is a SingleValue question. Use exactly one measure. \
            dimensions MUST be empty.\n\
            - time_dimensions: if the question implies a date scope, add the date column with \
            granularity: null and the appropriate date_range. This filters without grouping.\n\
            - order: leave as an empty array — a single aggregate row has no sort order.\n\
            </question_type_guidance>"
        }
        QuestionType::Distribution => {
            "\n<question_type_guidance>\n\
            This is a Distribution question. Use one measure for the value being distributed. \
            Use the question context to decide between raw values and histogram grouping.\n\
            - order: leave as an empty array for raw values; for histograms order by bucket ascending.\n\
            </question_type_guidance>"
        }
        QuestionType::GeneralInquiry => unreachable!("GeneralInquiry must not reach specify_impl"),
    }
}

// ---------------------------------------------------------------------------
// Solve
// ---------------------------------------------------------------------------

/// System prompt base for the **Solve** stage.
///
/// Combined at runtime with a per-question-type addendum via [`solve_type_addendum`].
pub(super) const SOLVE_BASE_PROMPT: &str = "\
<role>
You are a SQL expert. Given a structured analytics spec, write a single executable SQL query.
</role>

<constraints>
- You MUST call the solve_response tool to return your final SQL. Do NOT write the SQL \
directly in your text response — always use the tool.
- Reference only the tables listed in the spec.
- Follow the join path exactly as specified.
- Apply all resolved filters verbatim in the WHERE clause — they are already fully qualified (table.column).
- Use standard SQL syntax compatible with most ANSI-compliant databases.
- The number of columns in your SELECT MUST match the expected result shape exactly.
</constraints>

<tools>
Use execute_preview(sql) to verify your SQL before finalizing:
- It runs your query with LIMIT 5 and returns real columns and rows.
- If the query has a syntax error or references a missing table/column, it returns {ok: false, error: ...}.
- Use it to check that joins and filters produce real rows, not to count results.
- Call it at most once before submitting your final SQL.
</tools>

<result_shape_rules>
The expected result shape controls the SELECT clause structure:
- Scalar: SELECT exactly ONE aggregate expression. No GROUP BY. Returns 1 row \u{00d7} 1 column.
- Series: SELECT exactly ONE column or expression. No date column, no grouping dims in SELECT. \
Date filters go in WHERE only. Example: SELECT weight FROM body_composition WHERE date >= '2024-01-01'
- Table: SELECT the metric(s) AND all grouping dimension columns. Include GROUP BY for dims.
- TimeSeries: SELECT a date/time column AND the metric(s). Include GROUP BY date if aggregating.
</result_shape_rules>";

pub(super) fn solve_type_addendum(question_type: &QuestionType) -> &'static str {
    match question_type {
        QuestionType::Trend => {
            "\n<sql_pattern>\n\
            Trend query: SELECT date_col, aggregate(metric) ... GROUP BY date_col ORDER BY date_col ASC.\n\
            Use DATE_TRUNC or strftime for granularity (daily for short ranges, monthly for long).\n\
            Result shape: TimeSeries \u{2014} SELECT a date/time column AND the metric(s). GROUP BY date if aggregating.\n\
            </sql_pattern>"
        }
        QuestionType::Comparison => {
            "\n<sql_pattern>\n\
            Comparison query: SELECT comparison_dim, aggregate(metric) ... GROUP BY comparison_dim.\n\
            For time-period comparisons use CASE WHEN or UNION to label periods.\n\
            Result shape: Table \u{2014} SELECT the metric(s) AND all grouping dimension columns. Include GROUP BY for dims.\n\
            </sql_pattern>"
        }
        QuestionType::Breakdown => {
            "\n<sql_pattern>\n\
            Breakdown query: SELECT grouping_dim(s), aggregate(metric) ... GROUP BY grouping_dim(s) ORDER BY aggregate DESC.\n\
            Result shape: Table \u{2014} SELECT the metric(s) AND all grouping dimension columns. Include GROUP BY for dims.\n\
            </sql_pattern>"
        }
        QuestionType::SingleValue => {
            "\n<sql_pattern>\n\
            SingleValue query: SELECT aggregate(metric) ... WHERE scope_filters. No GROUP BY. 1 row \u{00d7} 1 col.\n\
            Result shape: Scalar \u{2014} SELECT exactly ONE aggregate expression. No GROUP BY. Returns 1 row \u{00d7} 1 column.\n\
            </sql_pattern>"
        }
        QuestionType::Distribution => {
            "\n<sql_pattern>\n\
            Distribution query: Raw values: SELECT metric_column ... ; \
            Histogram: SELECT bucket_expr AS bucket, COUNT(*) ... GROUP BY bucket ORDER BY bucket.\n\
            Result shape: Series (raw values, 1 column) or Table (histogram with bucket + count columns).\n\
            </sql_pattern>"
        }
        // GeneralInquiry is short-circuited before solve_impl is called.
        QuestionType::GeneralInquiry => unreachable!("GeneralInquiry must not reach solve_impl"),
    }
}

// ---------------------------------------------------------------------------
// General Inquiry
// ---------------------------------------------------------------------------

/// System prompt for the **GeneralInquiry** short-circuit path.
pub(super) const GENERAL_INQUIRY_SYSTEM_PROMPT: &str = "\
<role>
You are a helpful analytics assistant. The user has asked a general question that \
does not require querying data — for example, asking what tables or metrics are \
available, how the system works, or what kinds of questions it can answer.
</role>

<guidelines>
- Be concise and direct.
- If the user asks about available tables or metrics, enumerate them clearly from \
the schema context provided.
- If the user asks about your capabilities, explain what kinds of analytical \
questions you can answer (trends over time, comparisons, breakdowns by category, \
single aggregate values, distributions).
- Do not fabricate tables, metrics, or columns that are not present in the schema.
- Do not mention SQL or internal implementation details.
</guidelines>";

// ---------------------------------------------------------------------------
// Interpret
// ---------------------------------------------------------------------------

/// Additional instructions appended to `INTERPRET_SYSTEM_PROMPT` when the
/// result contains multiple independent query outputs (fan-out).
/// Each result set is labelled "Result set N (result_index: N-1)" in the prompt
/// and carries its own columns; use `result_index` in `render_chart` to target
/// the correct one.
pub(super) const MULTI_RESULT_INTERPRET_ADDON: &str = "\
<multi_result>
The query results contain data from MULTIPLE INDEPENDENT QUERIES, each shown as \
a separate \"Result set N\" block with its own columns.
Write a single cohesive response that addresses ALL parts of the original question.
- Interpret each result set in context of its sub-question.
- Draw connections between datasets when relevant.
- When rendering charts, set `result_index` to the 0-based index matching the \
  \"Result set N\" block whose columns you are referencing.
- Do NOT mention that queries were split internally.
- Preserve all specific data values; never invent numbers.
</multi_result>";

/// System prompt for the **Interpret** stage.
pub(super) const INTERPRET_SYSTEM_PROMPT: &str = "\
<role>
You are an analytics expert. Given a question and query results, write a clear, \
data-driven natural-language answer that synthesizes the actual numbers. \
When the data is best understood visually, also call the render_chart tool to \
produce a chart alongside your answer.
</role>

<constraints>
- Do not describe the SQL or methodology \u{2014} answer the question directly.
- Lead with the key finding stated as a concrete fact with numbers.
- Call render_chart at most once per response; do not call it for scalar answers.
</constraints>

<guidelines>
- Synthesize the data: call out specific values, totals, averages, percentages, \
rankings, and notable patterns directly from the result rows.
- Compare and contrast: highlight highest vs lowest, biggest changes, or most \
notable differences.
- Quantify proportions: express values as percentages of the total when relevant.
- If the data has time-based patterns, call out specific dates/periods and their \
values rather than vague terms like \"sharp drop-off\".
</guidelines>

<chart_guidelines>
Call render_chart when the question involves:
- Trends over time (line_chart: x = date/time column, y = metric column)
- Comparisons across categories (bar_chart: x = category column, y = metric column)
- Breakdowns by a dimension (bar_chart or pie_chart depending on the number of groups; \
  use pie_chart only when there are \u{2264} 8 slices and the values sum to a meaningful whole)
- Distributions (bar_chart: x = bucket/category, y = count or frequency)

Do NOT call render_chart for:
- Single scalar results (one number)
- General-inquiry answers that have no tabular data
- Multi-result merged sets (the \"result_set\" column is present)

Column mapping rules:
- Use EXACT column names from the query result (case-sensitive).
- For line_chart / bar_chart: set x to the dimension/date column and y to the \
  numeric metric column.  Set series only when there is a third grouping column \
  whose distinct values should become separate lines or bars (e.g. x='month', \
  y='revenue', series='region' produces one line/bar per region).  When there is \
  no grouping column, set series to null — a single series is rendered automatically.
- For pie_chart: set name to the category column and value to the numeric column. \
  Do NOT set x, y, or series — they are ignored for pie charts.
- For table: no column mapping needed — all columns are displayed automatically.
- Always set title to a concise description of what the chart shows.
- Always set x_axis_label and y_axis_label for line_chart and bar_chart to human-readable \
  labels (e.g. \"Date\", \"Revenue (USD)\", \"Number of Orders\") so the chart is \
  self-explanatory.  Include units where applicable (e.g. \"Sales ($)\", \"Duration (ms)\").
- For pie_chart, set title so the chart is self-explanatory without additional labels.

Suggested and previous chart configs may be provided in the query results section:
- \"Suggested chart config\" \u{2014} auto-computed from the result shape; use it as a \
  reference for column names and chart type.  You are free to deviate if a different \
  visualization better answers the question.
- \"Previous chart config\" \u{2014} the chart rendered in the previous turn.  When the \
  user asks to edit or change the chart (e.g. \"show as a bar chart\", \"add a title\"), \
  call render_chart with the adjusted fields carried over from the previous config.
</chart_guidelines>

<table_formatting>
- When result has 2+ rows AND 2+ columns, INCLUDE a markdown table \
(| Col1 | Col2 |, then |---|---|, then data rows).
- Round numbers to 1-2 decimal places.
- After the table, add 1-2 sentences highlighting the most important takeaway.
- For scalar results, do NOT use a table \u{2014} just state the answer directly.
- If more than 15 rows, show top 10 and note how many were omitted.
</table_formatting>";

// ---------------------------------------------------------------------------
// Shared formatting helpers
// ---------------------------------------------------------------------------

/// Format a retry context block to append to LLM prompts on back-edges.
///
/// Returns an empty string when `retry_ctx` is `None` or has no useful content,
/// so callers can unconditionally append it without extra branches.
pub(super) fn format_retry_section(retry_ctx: Option<&RetryContext>) -> String {
    let Some(ctx) = retry_ctx else {
        return String::new();
    };
    if ctx.attempt == 0 && ctx.errors.is_empty() {
        return String::new();
    }
    let mut parts = vec![format!("\n\nAttempt: {}", ctx.attempt + 1)];
    if !ctx.errors.is_empty() {
        parts.push(format!(
            "Prior errors (do NOT repeat these mistakes):\n{}",
            ctx.errors
                .iter()
                .enumerate()
                .map(|(i, e)| format!("  {}. {e}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if let Some(prev) = &ctx.previous_output {
        parts.push(format!("Previous output:\n{prev}"));
    }
    parts.join("\n")
}

/// Format a "Previously confirmed" block for the Specify prompt when a
/// `SpecHint` (airlayer `QueryRequestItem`) is present on the intent.
///
/// Serializes the prior query as JSON so the LLM can see the exact airlayer
/// grammar used previously.  Returns an empty string when there is no hint
/// or the hint is entirely empty, so callers can unconditionally append it.
pub(super) fn format_spec_hint_section(hint: Option<&SpecHint>) -> String {
    let Some(h) = hint else { return String::new() };
    if h.measures.is_empty() && h.dimensions.is_empty() && h.filters.is_empty() {
        return String::new();
    }
    let json = serde_json::to_string_pretty(h).unwrap_or_default();
    format!(
        "\n\nPrevious query (use as a starting point — adjust only what the new question requires):\n\
         ```json\n{json}\n```"
    )
}

/// Format prior conversation turns as a context prefix for LLM prompts.
///
/// Returns an empty string when there is no history, so callers can
/// unconditionally prepend it without extra branches.
pub(super) fn format_history_section(history: &[ConversationTurn]) -> String {
    if history.is_empty() {
        return String::new();
    }
    let turns: Vec<String> = history
        .iter()
        .map(|t| format!("Q: {}\nA: {}", t.question.trim(), t.answer.trim()))
        .collect();
    format!("Prior conversation:\n{}\n\n", turns.join("\n\n"))
}

/// Format completed session turns as a preceding conversation block for LLM prompts.
///
/// Injects all retained turns (up to `max_turns`) into Clarifying prompts so
/// the LLM can resolve pronoun/reference ambiguity across the full session.
/// Returns an empty string when there are no prior turns.
pub(super) fn format_session_turns_section(turns: &[CompletedTurn<AnalyticsDomain>]) -> String {
    if turns.is_empty() {
        return String::new();
    }
    let mut parts = vec!["Previous conversation:".to_string()];
    for (i, turn) in turns.iter().enumerate() {
        parts.push(format!(
            "Turn {n}:\n  User: {q}\n  Assistant: {a}",
            n = i + 1,
            q = turn.intent.raw_question.trim(),
            a = turn.answer.text.trim(),
        ));
    }
    format!("{}\n\n", parts.join("\n"))
}
