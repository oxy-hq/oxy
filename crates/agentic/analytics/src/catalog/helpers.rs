//! Module-level helpers for [`SchemaCatalog`]: type inference and keyword heuristics.
//!
//! [`SchemaCatalog`]: super::SchemaCatalog

/// Convert a [`CellValue`] from schema introspection to a JSON value.
///
/// [`CellValue`]: agentic_core::result::CellValue
pub(super) fn cell_to_json(v: &agentic_core::result::CellValue) -> Option<serde_json::Value> {
    use agentic_core::result::CellValue;
    match v {
        CellValue::Text(s) => Some(serde_json::Value::String(s.clone())),
        CellValue::Number(f) => Some(serde_json::json!(f)),
        CellValue::Null => None,
    }
}

/// Map a database-native type string to the semantic type used by the catalog.
///
/// Returns `None` when unrecognised — the caller falls back to the
/// column-name heuristic ([`type_hint`]).
pub(super) fn db_type_to_semantic(db_type: &str) -> Option<&'static str> {
    let t = db_type.to_uppercase();
    // ── Temporal (checked before numeric so "INTERVAL" doesn't match "INT") ─
    if t.starts_with("DATE")
        || t.starts_with("TIME")
        || t.starts_with("TIMESTAMP")
        || t.starts_with("DATETIME")
        || t.starts_with("INTERVAL")
    {
        return Some("date");
    }
    // ── Numeric ──────────────────────────────────────────────────────────
    if t.starts_with("INT")
        || t.starts_with("TINYINT")
        || t.starts_with("SMALLINT")
        || t.starts_with("BIGINT")
        || t.starts_with("HUGEINT")
        || t.starts_with("FLOAT")
        || t.starts_with("DOUBLE")
        || t.starts_with("DECIMAL")
        || t.starts_with("NUMERIC")
        || t.starts_with("REAL")
        || t.starts_with("NUMBER")
    {
        return Some("number");
    }
    // ── Boolean ───────────────────────────────────────────────────────────
    if t.starts_with("BOOL") {
        return Some("boolean");
    }
    // ── String ────────────────────────────────────────────────────────────
    if t.starts_with("VARCHAR")
        || t.starts_with("CHAR")
        || t.starts_with("TEXT")
        || t.starts_with("STRING")
        || t.starts_with("CLOB")
        || t.starts_with("ENUM")
        || t.starts_with("BLOB")
        || t.starts_with("BYTES")
    {
        return Some("string");
    }
    None
}

/// Column-name fragments that suggest a numeric metric.
pub(super) const METRIC_KEYWORDS: &[&str] = &[
    "amount",
    "total",
    "count",
    "revenue",
    "cost",
    "price",
    "sum",
    "avg",
    "average",
    "qty",
    "quantity",
    "volume",
    "sales",
    "profit",
    "margin",
    "rate",
    "score",
    "weight",
    "calories",
    "reps",
    "sets",
    "duration",
    "distance",
    "speed",
    "heart",
    "fat",
    "protein",
    "carbs",
    "incline",
    "elevation",
    "rpe",
    "stiffness",
    "max",
    "min",
    "percent",
    "pct",
    "ratio",
];

/// Returns `true` when `col` looks like an identifier or foreign key.
pub(super) fn is_id_col(col: &str) -> bool {
    let l = col.to_lowercase();
    l == "id" || l.ends_with("_id") || l.starts_with("id_")
}

/// Column-name fragments that suggest a date/time dimension.
pub(super) const DATE_KEYWORDS: &[&str] = &[
    "date",
    "time",
    "day",
    "month",
    "year",
    "created_at",
    "updated_at",
    "timestamp",
];

pub(super) fn is_metric_col(col: &str) -> bool {
    let l = col.to_lowercase();
    METRIC_KEYWORDS.iter().any(|kw| l.contains(kw))
}

pub(super) fn is_date_col(col: &str) -> bool {
    let l = col.to_lowercase();
    DATE_KEYWORDS.iter().any(|kw| l.contains(kw))
}

pub(super) fn type_hint(col: &str) -> &'static str {
    if is_date_col(col) {
        "date"
    } else if is_metric_col(col) {
        "number"
    } else {
        "string"
    }
}
