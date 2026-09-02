//! Airlayer-native query request types consumed by the Specifying LLM response.

use serde::{Deserialize, Serialize};

/// Confirmed query structure from a prior Specifying attempt.
///
/// Uses the airlayer `QueryRequestItem` grammar (measures, dimensions,
/// filters, time_dimensions, order, limit) so the LLM can reuse the
/// prior query structure on back-edge retries and cross-turn follow-ups.
pub type SpecHint = QueryRequestItem;

// Airlayer-native query request types (LLM response deserialization)

/// Top-level envelope for the airlayer-native Specify response.
///
/// The LLM returns one or more `QueryRequestItem` specs, each of which can
/// be independently compiled via `oxy_airlayer_compat::SemanticEngine::compile_query`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequestEnvelope {
    pub specs: Vec<QueryRequestItem>,
}

/// A single query spec in airlayer-native format.
///
/// Mirrors `oxy_airlayer_compat::engine::query::QueryRequest` but includes an
/// `assumptions` field for human review and uses owned deserialization types.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryRequestItem {
    /// Measure members to aggregate (e.g. `["orders.total_revenue"]`).
    #[serde(default)]
    pub measures: Vec<String>,
    /// Non-time dimension members to group by (e.g. `["orders.status"]`).
    #[serde(default)]
    pub dimensions: Vec<String>,
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
    pub fn to_query_request(&self) -> oxy_airlayer_compat::engine::query::QueryRequest {
        use oxy_airlayer_compat::engine::query::{
            OrderBy, QueryFilter, QueryRequest, TimeDimensionQuery,
        };

        let filters = self
            .filters
            .iter()
            // An unknown operator DROPS its filter rather than guessing one.
            // Both outcomes are wrong, so this picks the one that fails loudly:
            // a widened result reads as suspicious, while the narrowing guess it
            // replaced produced an empty result that reads as a confident answer.
            //
            // Unreachable now that BOTH producers gate on `query_problems` —
            // `propose_semantic_query` and `parse_query_request_response`. It
            // was reachable when only the first did, and this arm is what made
            // that hole look benign: the query still ran, just without the
            // filter. So the `error!` means a third producer appeared without a
            // gate, which is the thing to go and look at.
            .filter_map(|f| {
                let operator = parse_filter_operator(&f.operator).or_else(|| {
                    tracing::error!(
                        operator = %f.operator,
                        member = %f.member,
                        "unknown semantic filter operator reached compilation; dropping the filter"
                    );
                    None
                })?;
                Some(QueryFilter {
                    member: Some(f.member.clone()),
                    operator: Some(operator),
                    values: f.values.clone(),
                    and: None,
                    or: None,
                })
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

/// Every operator the semantic model understands, for the message a rejection
/// shows the model. Same order as [`parse_filter_operator`].
pub const FILTER_OPERATORS: &[&str] = &[
    "equals",
    "notEquals",
    "contains",
    "notContains",
    "startsWith",
    "notStartsWith",
    "endsWith",
    "notEndsWith",
    "gt",
    "gte",
    "lt",
    "lte",
    "set",
    "notSet",
    "inDateRange",
    "notInDateRange",
    "beforeDate",
    "beforeOrOnDate",
    "afterDate",
    "afterOrOnDate",
];

/// Whether the semantic model can compile this operator.
///
/// Used to REJECT a proposed query before it runs — see the note on
/// [`parse_filter_operator`] for why an unknown operator must not be guessed at.
pub fn is_known_filter_operator(s: &str) -> bool {
    parse_filter_operator(s).is_some()
}

/// Why this filter cannot be compiled, or `None` if it can.
///
/// Arity as well as the operator name, because the two fail the same way. A
/// model that had `inDateRange` rejected for arity-free reasons came back with
/// `inDateRange` and ONE value, which compiled to `>= d AND <= d` — a
/// single-day range presented as a week, answered as "no sales last week".
/// Every silent narrowing on this path ends in a confident empty answer, so
/// the arity is checked where the operator is.
pub fn filter_problem(operator: &str, values: &[String]) -> Option<String> {
    if !is_known_filter_operator(operator) {
        return Some(format!("unknown filter operator `{operator}`"));
    }
    match operator {
        "inDateRange" | "notInDateRange" if values.len() != 2 => Some(format!(
            "`{operator}` needs exactly 2 values [start, end]; got {}",
            values.len()
        )),
        "set" | "notSet" if !values.is_empty() => Some(format!(
            "`{operator}` takes no values; got {}",
            values.len()
        )),
        "set" | "notSet" => None,
        _ if values.is_empty() => Some(format!("`{operator}` needs at least one value")),
        _ => None,
    }
}

/// Everything about `item` the semantic model cannot compile, as messages for
/// the model.
///
/// **One function, because there are two producers of a `QueryRequestItem` and
/// the first version of this gate only guarded one.** `propose_semantic_query`
/// (the clarifying shortcut) was checked; `parse_query_request_response` (the
/// main specify path) was not, and that path is the common one. Worse, the two
/// halves failed in opposite directions: the gated path narrowed to nothing,
/// while the ungated one reached [`QueryRequestItem::to_query_request`], whose
/// `filter_map` DROPS an uncompilable filter — so "revenue last week" answered
/// with all-time revenue. Both are wrong answers nobody can see; neither is
/// worth a second copy of the rules.
///
/// Empty means the item is compilable.
///
/// **This catches malformed shapes, not wrong answers.** A model that resolves
/// "last week" to `inDateRange ["2026-08-14", "2026-08-14"]` produces a
/// perfectly valid one-day window, and no validator can tell that apart from
/// someone legitimately asking about the 14th. Four spellings of the date bug
/// have been seen; three are structurally impossible to mean what they say and
/// are rejected here, the fourth is the model choosing badly and belongs to the
/// prompt, not to this gate. Do not grow a rule for it — rejecting `start ==
/// end` would break single-day queries, which are real.
pub fn query_problems(item: &QueryRequestItem) -> Vec<String> {
    let mut out: Vec<String> = item
        .filters
        .iter()
        .filter_map(|f| {
            filter_problem(&f.operator, &f.values).map(|why| format!("{} on `{}`", why, f.member))
        })
        .collect();
    // A `date_range` is the same trap as `inDateRange` with one value, one
    // field over: `["2026-08-13"]` compiles to that single day and the week
    // reads as empty. Checked with the filters because it is the shape a model
    // reaches for INSTEAD of one, which is exactly how gating only filters left
    // the hole open.
    out.extend(item.time_dimensions.iter().filter_map(|td| {
        match td.date_range.as_ref().map(Vec::len) {
            Some(n) if n != 2 => Some(format!(
                "`date_range` on `{}` needs exactly 2 values [start, end]; got {n}",
                td.dimension
            )),
            _ => None,
        }
    }));
    out.extend(unsatisfiable_equals(&item.filters));
    out
}

/// Members carrying two or more `equals` filters whose value sets do not
/// intersect.
///
/// Filters are ANDed, so `business_date = '08-14' AND business_date = '08-15'`
/// can never match a row. A model asked for "last week" produced exactly that —
/// six `equals` filters, one per day — and every one of them is individually
/// valid, so the operator and arity checks passed it straight through to a
/// confident "no sales last week". Third spelling of the same failure, and the
/// first two gates could not see it because nothing about any single filter is
/// wrong.
///
/// Only `equals` is considered. `gt` + `lt` on one member is a legitimate
/// range, and `notEquals` narrows without contradicting.
fn unsatisfiable_equals(filters: &[StructuredFilter]) -> Vec<String> {
    use std::collections::HashMap;
    let mut by_member: HashMap<&str, Vec<&StructuredFilter>> = HashMap::new();
    for f in filters.iter().filter(|f| f.operator == "equals") {
        by_member.entry(f.member.as_str()).or_default().push(f);
    }
    by_member
        .into_iter()
        .filter(|(_, fs)| fs.len() > 1)
        .filter_map(|(member, fs)| {
            // `equals` with several values already means IN, so the AND of two
            // of them is the intersection. Empty means no row can qualify.
            let mut common: std::collections::BTreeSet<&str> =
                fs[0].values.iter().map(String::as_str).collect();
            for f in &fs[1..] {
                let next: std::collections::BTreeSet<&str> =
                    f.values.iter().map(String::as_str).collect();
                common = common.intersection(&next).copied().collect();
            }
            common.is_empty().then(|| {
                format!(
                    "`{member}` has {} `equals` filters with no value in common — they are \
                     ANDed, so nothing can match. Use one `inDateRange` [start, end] for a \
                     span, or one `equals` listing every value",
                    fs.len()
                )
            })
        })
        .collect()
}

/// The sentence appended to every rejection, so the model is told what to do
/// rather than only what was wrong.
pub const QUERY_REJECTION_HINT: &str = "For a date range use `inDateRange` with EXACTLY two values [start, end], or a \
     time_dimensions `date_range` with two — do not invent an operator, and do not pass \
     one value.";

/// Parse a camelCase operator string into an airlayer `FilterOperator`.
///
/// `None` for anything unrecognised, and **that matters**: this used to fall
/// back to `Equals`, which with two values compiles to `IN (a, b)`. So a model
/// that invented `last_week` with `["2026-08-13", "2026-08-19"]` — the two ends
/// of a range — got `business_date IN ('2026-08-13', '2026-08-19')`: every row
/// strictly inside the week silently dropped, and the agent reported "no sales
/// last week" with full confidence. A confident empty answer is the worst
/// possible failure for an analytics agent, so the operator is now validated
/// where the query is proposed and the model is asked to try again.
fn parse_filter_operator(s: &str) -> Option<oxy_airlayer_compat::engine::query::FilterOperator> {
    use oxy_airlayer_compat::engine::query::FilterOperator;
    Some(match s {
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
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The operator names `parse_filter_operator` actually accepts, read out
    /// of its own match arms.
    fn arms_of_parse_filter_operator() -> Vec<String> {
        let src = include_str!("query_request.rs");
        let body = src
            .split_once("fn parse_filter_operator")
            .expect("parse_filter_operator was renamed — this guard reads it by name")
            .1;
        let body = body.split_once("\n}\n").expect("unterminated fn").0;
        let arms: Vec<String> = body
            .lines()
            .filter_map(|l| {
                let (lit, _) = l.trim().split_once(" => FilterOperator::")?;
                Some(lit.trim().trim_matches('"').to_string())
            })
            .collect();
        assert!(
            arms.len() > 5,
            "the arm scan matched {} arms — the match syntax changed and this \
             guard is now vacuous",
            arms.len()
        );
        arms
    }

    /// `FILTER_OPERATORS` is what a rejected model is TOLD it may use, and
    /// `parse_filter_operator` is what actually compiles. Drift between them
    /// is silent and lands as a wrong answer either way: an operator listed
    /// but unparsed sends the model into a rejection loop it cannot escape,
    /// and one parsed but unlisted is a capability the model never learns it
    /// has — it substitutes something coarser and answers confidently.
    ///
    /// So this is checked in BOTH directions against the match arms
    /// themselves, not against a second hand-written list.
    #[test]
    fn the_advertised_operators_are_exactly_the_parsable_ones() {
        let arms = arms_of_parse_filter_operator();
        for op in FILTER_OPERATORS {
            assert!(
                arms.contains(&op.to_string()),
                "`{op}` is advertised but `parse_filter_operator` rejects it"
            );
        }
        for arm in &arms {
            assert!(
                FILTER_OPERATORS.contains(&arm.as_str()),
                "`{arm}` compiles but is never advertised — the model cannot \
                 use an operator it is not told about"
            );
        }
        assert_eq!(
            arms.len(),
            FILTER_OPERATORS.len(),
            "duplicate entry on one side"
        );
    }

    /// The order the doc comment on [`FILTER_OPERATORS`] claims.
    #[test]
    fn the_advertised_order_matches_the_match() {
        assert_eq!(arms_of_parse_filter_operator(), FILTER_OPERATORS);
    }

    fn item_with(filters: Vec<StructuredFilter>, tds: Vec<TimeDimensionItem>) -> QueryRequestItem {
        QueryRequestItem {
            measures: vec!["toast_sales.net_sales".into()],
            filters,
            time_dimensions: tds,
            ..Default::default()
        }
    }

    fn filter(operator: &str, values: &[&str]) -> StructuredFilter {
        StructuredFilter {
            member: "toast_sales.business_date".into(),
            operator: operator.into(),
            values: values.iter().map(|v| (*v).to_string()).collect(),
        }
    }

    fn time_dim(range: Option<Vec<&str>>) -> TimeDimensionItem {
        TimeDimensionItem {
            dimension: "toast_sales.business_date".into(),
            granularity: Some("day".into()),
            date_range: range.map(|r| r.iter().map(|v| (*v).to_string()).collect()),
        }
    }

    /// Each of these produced a confident WRONG answer before the gate existed,
    /// and the two failure directions are why both are listed: an invented
    /// operator narrowed to `IN (start, end)` on the shortcut path and *widened*
    /// to no filter at all on the specify path.
    #[test]
    fn a_query_the_semantic_layer_cannot_compile_is_rejected() {
        // The exact shape a model produced: `last_week` with the two ends of the
        // range. `Equals` with two values compiles to `IN (a, b)`.
        let invented = item_with(
            vec![filter("last_week", &["2026-08-13", "2026-08-19"])],
            vec![],
        );
        assert!(
            query_problems(&invented)
                .iter()
                .any(|p| p.contains("last_week")),
            "got {:?}",
            query_problems(&invented)
        );

        // Its retry: a real operator, one value, compiling to `>= d AND <= d`.
        let one_value = item_with(vec![filter("inDateRange", &["2026-08-13"])], vec![]);
        assert!(
            query_problems(&one_value)
                .iter()
                .any(|p| p.contains("exactly 2 values")),
            "got {:?}",
            query_problems(&one_value)
        );

        // Six `equals`, one per day of "last week". Each is individually
        // valid — which is why the operator and arity checks passed it — but
        // they are ANDed, so no row can match and the agent answered "no
        // sales". Found by running the question, not by reading.
        let per_day = item_with(
            (14..=19)
                .map(|d| filter("equals", &[&format!("2026-08-{d}")]))
                .collect(),
            vec![],
        );
        assert!(
            query_problems(&per_day)
                .iter()
                .any(|p| p.contains("no value in common")),
            "got {:?}",
            query_problems(&per_day)
        );

        // The same trap one field over — not a filter at all, which is how it
        // survived the first version of this gate.
        let short_range = item_with(vec![], vec![time_dim(Some(vec!["2026-08-13"]))]);
        assert!(
            query_problems(&short_range)
                .iter()
                .any(|p| p.contains("date_range")),
            "got {:?}",
            query_problems(&short_range)
        );
    }

    #[test]
    fn a_compilable_query_is_left_alone() {
        let ok = item_with(
            vec![filter("inDateRange", &["2026-08-13", "2026-08-19"])],
            vec![time_dim(Some(vec!["2026-08-13", "2026-08-19"]))],
        );
        assert!(
            query_problems(&ok).is_empty(),
            "got {:?}",
            query_problems(&ok)
        );

        // No range at all is a whole-history query, which is a real request.
        let no_range = item_with(vec![], vec![time_dim(None)]);
        assert!(query_problems(&no_range).is_empty());

        // `set` / `notSet` take no values, and requiring one would reject them.
        let unary = item_with(vec![filter("set", &[])], vec![]);
        assert!(
            query_problems(&unary).is_empty(),
            "got {:?}",
            query_problems(&unary)
        );

        // A range built from two DIFFERENT operators is the correct shape and
        // must not trip the contradiction check.
        let range = item_with(
            vec![
                filter("gte", &["2026-08-13"]),
                filter("lte", &["2026-08-19"]),
            ],
            vec![],
        );
        assert!(
            query_problems(&range).is_empty(),
            "got {:?}",
            query_problems(&range)
        );

        // Two `equals` that DO overlap are satisfiable — redundant, not wrong.
        let overlapping = item_with(
            vec![filter("equals", &["a", "b"]), filter("equals", &["b", "c"])],
            vec![],
        );
        assert!(
            query_problems(&overlapping).is_empty(),
            "got {:?}",
            query_problems(&overlapping)
        );
    }
}
