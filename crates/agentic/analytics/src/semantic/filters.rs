//! Parse raw filter strings into structured airlayer [`QueryFilter`]s + SQL
//! formatting for the raw-schema translation path.
//!
//! [`QueryFilter`]: airlayer::engine::query::QueryFilter

use crate::catalog::CatalogError;

use super::SemanticCatalog;

impl SemanticCatalog {
    /// Parse raw filter strings from the intent into structured airlayer
    /// [`QueryFilter`](airlayer::engine::query::QueryFilter) objects.
    ///
    /// Supports simple comparison filters like `"date >= '2024-01-01'"` or
    /// `"status = 'active'"`.  The column name is qualified against the
    /// semantic layer using `find_dimension` / `find_measure`, preferring
    /// `preferred_views` (the views already selected for metrics).
    ///
    /// Returns `Err(TooComplex)` when a filter contains SQL functions or
    /// expressions that cannot be represented in airlayer's filter API.
    pub(super) fn parse_intent_filters(
        &self,
        filters: &[String],
        preferred_views: &[String],
    ) -> Result<Vec<airlayer::engine::query::QueryFilter>, CatalogError> {
        filters
            .iter()
            .map(|raw| {
                Self::parse_single_filter(raw, |col| {
                    self.qualify_filter_column(col, preferred_views)
                })
            })
            .collect()
    }

    /// Parse a single raw filter string into a [`QueryFilter`].
    ///
    /// Expected format: `<column> <op> <value>` where `<op>` is one of
    /// `=`, `!=`, `<>`, `>=`, `<=`, `>`, `<`, `IN`, `NOT IN`, `IS NULL`,
    /// `IS NOT NULL`, `LIKE`, `NOT LIKE`, `BETWEEN`.
    ///
    /// [`QueryFilter`]: airlayer::engine::query::QueryFilter
    fn parse_single_filter(
        raw: &str,
        qualify: impl Fn(&str) -> Option<String>,
    ) -> Result<airlayer::engine::query::QueryFilter, CatalogError> {
        use airlayer::engine::query::{FilterOperator, QueryFilter};

        let trimmed = raw.trim();
        let upper = trimmed.to_uppercase();

        // IS NULL / IS NOT NULL
        if upper.ends_with("IS NOT NULL") {
            let col = trimmed[..trimmed.len() - "IS NOT NULL".len()].trim();
            let member = qualify(col)
                .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
            return Ok(QueryFilter {
                member: Some(member),
                operator: Some(FilterOperator::Set),
                values: vec![],
                and: None,
                or: None,
            });
        }
        if upper.ends_with("IS NULL") {
            let col = trimmed[..trimmed.len() - "IS NULL".len()].trim();
            let member = qualify(col)
                .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
            return Ok(QueryFilter {
                member: Some(member),
                operator: Some(FilterOperator::NotSet),
                values: vec![],
                and: None,
                or: None,
            });
        }

        // BETWEEN ... AND ...
        if let Some(between_pos) = upper.find(" BETWEEN ") {
            let col = trimmed[..between_pos].trim();
            let rest = trimmed[between_pos + " BETWEEN ".len()..].trim();
            // Split on " AND " (case-insensitive)
            let rest_upper = rest.to_uppercase();
            if let Some(and_pos) = rest_upper.find(" AND ") {
                let lo = Self::strip_quotes(rest[..and_pos].trim());
                let hi = Self::strip_quotes(rest[and_pos + " AND ".len()..].trim());
                // Values containing SQL functions → too complex
                if Self::value_is_expression(&lo) || Self::value_is_expression(&hi) {
                    return Err(CatalogError::TooComplex(
                        "filter value contains SQL expression".into(),
                    ));
                }
                let member = qualify(col)
                    .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
                return Ok(QueryFilter {
                    member: Some(member),
                    operator: Some(FilterOperator::InDateRange),
                    values: vec![lo, hi],
                    and: None,
                    or: None,
                });
            }
            return Err(CatalogError::TooComplex("malformed BETWEEN filter".into()));
        }

        // Comparison operators (ordered longest-first to avoid prefix conflicts)
        let ops: &[(&str, FilterOperator)] = &[
            (">=", FilterOperator::Gte),
            ("<=", FilterOperator::Lte),
            ("!=", FilterOperator::NotEquals),
            ("<>", FilterOperator::NotEquals),
            (">", FilterOperator::Gt),
            ("<", FilterOperator::Lt),
            ("=", FilterOperator::Equals),
        ];

        for (op_str, op) in ops {
            if let Some(pos) = trimmed.find(op_str) {
                let col = trimmed[..pos].trim();
                let val = Self::strip_quotes(trimmed[pos + op_str.len()..].trim());
                if Self::value_is_expression(&val) {
                    return Err(CatalogError::TooComplex(
                        "filter value contains SQL expression".into(),
                    ));
                }
                let member = qualify(col)
                    .ok_or_else(|| CatalogError::TooComplex("unresolvable filter column".into()))?;
                return Ok(QueryFilter {
                    member: Some(member),
                    operator: Some(op.clone()),
                    values: vec![val],
                    and: None,
                    or: None,
                });
            }
        }

        // Could not parse → too complex for the semantic layer
        Err(CatalogError::TooComplex(format!(
            "unable to parse filter: {}",
            trimmed
        )))
    }

    /// Qualify a bare or dotted column name for use in a filter member.
    ///
    /// Tries `find_dimension` first (filters usually target dimensions),
    /// then `find_measure`.  `preferred_views` biases resolution toward
    /// views already selected for the current query's metrics.
    fn qualify_filter_column(&self, col: &str, preferred_views: &[String]) -> Option<String> {
        // Already qualified (view.column)
        if col.contains('.') {
            // Verify it resolves
            if self.find_dimension(col).is_some() || self.find_measure(col).is_some() {
                return Some(col.to_string());
            }
            return None;
        }

        // Try qualifying via qualify_names (reuses the same preference logic).
        self.qualify_names(&[col.to_string()], false, preferred_views)
            .and_then(|v| v.into_iter().next())
            .or_else(|| {
                // Fallback: try as a measure name
                self.qualify_names(&[col.to_string()], true, preferred_views)
                    .and_then(|v| v.into_iter().next())
            })
    }

    /// Strip surrounding single or double quotes from a value.
    fn strip_quotes(s: &str) -> String {
        let s = s.trim();
        if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }

    /// Return `true` when a filter value looks like a SQL expression rather
    /// than a simple literal (contains parentheses or SQL keywords).
    fn value_is_expression(val: &str) -> bool {
        let u = val.to_uppercase();
        val.contains('(')
            || u.contains("CURRENT_DATE")
            || u.contains("CURRENT_TIMESTAMP")
            || u.contains("NOW(")
            || u.contains("DATE_SUB")
            || u.contains("DATE_ADD")
            || u.contains("INTERVAL")
            || u.contains("SELECT")
    }

    /// Return `true` when any filter expression suggests window/CTE complexity.
    pub(super) fn filters_are_complex(filters: &[String]) -> bool {
        const COMPLEX: &[&str] = &[
            "OVER",
            "ROW_NUMBER",
            "RANK(",
            "DENSE_RANK",
            "LAG(",
            "LEAD(",
            "WITH ",
            "HAVING",
        ];
        filters.iter().any(|f| {
            let u = f.to_uppercase();
            COMPLEX.iter().any(|kw| u.contains(kw))
        })
    }
}

/// Format a structured filter into a raw SQL WHERE clause fragment.
pub(super) fn format_filter_as_sql(
    col_expr: &str,
    operator: &airlayer::engine::query::FilterOperator,
    values: &[String],
) -> String {
    use airlayer::engine::query::FilterOperator;
    match operator {
        FilterOperator::Equals if values.len() == 1 => {
            format!("{} = '{}'", col_expr, values[0])
        }
        FilterOperator::Equals => {
            let vals: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
            format!("{} IN ({})", col_expr, vals.join(", "))
        }
        FilterOperator::NotEquals if values.len() == 1 => {
            format!("{} != '{}'", col_expr, values[0])
        }
        FilterOperator::NotEquals => {
            let vals: Vec<String> = values.iter().map(|v| format!("'{v}'")).collect();
            format!("{} NOT IN ({})", col_expr, vals.join(", "))
        }
        FilterOperator::Contains if !values.is_empty() => {
            format!("{} LIKE '%{}%'", col_expr, values[0])
        }
        FilterOperator::NotContains if !values.is_empty() => {
            format!("{} NOT LIKE '%{}%'", col_expr, values[0])
        }
        FilterOperator::StartsWith if !values.is_empty() => {
            format!("{} LIKE '{}%'", col_expr, values[0])
        }
        FilterOperator::EndsWith if !values.is_empty() => {
            format!("{} LIKE '%{}'", col_expr, values[0])
        }
        FilterOperator::Gt if !values.is_empty() => {
            format!("{} > '{}'", col_expr, values[0])
        }
        FilterOperator::Gte if !values.is_empty() => {
            format!("{} >= '{}'", col_expr, values[0])
        }
        FilterOperator::Lt if !values.is_empty() => {
            format!("{} < '{}'", col_expr, values[0])
        }
        FilterOperator::Lte if !values.is_empty() => {
            format!("{} <= '{}'", col_expr, values[0])
        }
        FilterOperator::Set => format!("{} IS NOT NULL", col_expr),
        FilterOperator::NotSet => format!("{} IS NULL", col_expr),
        FilterOperator::InDateRange if values.len() == 2 => {
            format!(
                "{} >= '{}' AND {} < '{}'",
                col_expr, values[0], col_expr, values[1]
            )
        }
        FilterOperator::NotInDateRange if values.len() == 2 => {
            format!(
                "({} < '{}' OR {} >= '{}')",
                col_expr, values[0], col_expr, values[1]
            )
        }
        FilterOperator::BeforeDate if !values.is_empty() => {
            format!("{} < '{}'", col_expr, values[0])
        }
        FilterOperator::AfterDate if !values.is_empty() => {
            format!("{} > '{}'", col_expr, values[0])
        }
        FilterOperator::BeforeOrOnDate if !values.is_empty() => {
            format!("{} <= '{}'", col_expr, values[0])
        }
        FilterOperator::AfterOrOnDate if !values.is_empty() => {
            format!("{} >= '{}'", col_expr, values[0])
        }
        // Fallback
        _ => format!(
            "{} = '{}'",
            col_expr,
            values.first().unwrap_or(&String::new())
        ),
    }
}
