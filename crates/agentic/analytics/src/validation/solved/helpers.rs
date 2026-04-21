//! Shared predicates used by multiple solved-stage rules.

use crate::ResultShape;

/// Return `true` when `s` looks like a date or timestamp string.
///
/// Recognises:
/// - ISO date: `YYYY-MM-DD` or partial `YYYY-MM`
/// - ISO week: `YYYY-W##` or `YYYY-W##-D`
/// - Weekday names: Monday … Sunday (full or 3-letter abbreviation)
///
/// Bug-fix #5: the original implementation also accepted any 8-digit pure
/// numeric string as a "Unix timestamp".  This was too loose — order IDs,
/// customer IDs, and SKUs are commonly 8-digit integers and would falsely
/// satisfy the TimeSeries date check.  Numeric timestamps stored as integers
/// appear as `CellValue::Number`, not `CellValue::Text`, so the string path
/// never needs to handle them.
pub fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    // YYYY-MM or YYYY-MM-DD: 4 digits, dash, 2 digits
    if b.len() >= 7
        && b[0..4].iter().all(|x| x.is_ascii_digit())
        && b[4] == b'-'
        && b[5..7].iter().all(|x| x.is_ascii_digit())
    {
        return true;
    }
    // YYYY-W## ISO week or YYYY-W##-D ISO weekday format
    if b.len() >= 7
        && b[0..4].iter().all(|x| x.is_ascii_digit())
        && b[4] == b'-'
        && b[5] == b'W'
        && b.len() > 6
        && b[6..].iter().all(|x| x.is_ascii_digit() || *x == b'-')
    {
        return true;
    }
    // Weekday names (full or 3-letter abbreviation, case-insensitive)
    matches!(
        s.to_ascii_lowercase().as_str(),
        "monday"
            | "tuesday"
            | "wednesday"
            | "thursday"
            | "friday"
            | "saturday"
            | "sunday"
            | "mon"
            | "tue"
            | "wed"
            | "thu"
            | "fri"
            | "sat"
            | "sun"
    )
}

/// Infer the [`ResultShape`] from row/column counts for mismatch error reports.
///
/// Returns only `Scalar`, `Series`, or `Table` — never `TimeSeries`.
/// Time-series data is structurally a table (dimensions + metrics); treating
/// it as a separate shape caused false validation failures when the expected
/// shape was `Table` but the actual data contained date-like columns.
pub fn infer_shape(
    total_row_count: u64,
    columns: &[String],
    _rows: &[agentic_core::QueryRow],
) -> ResultShape {
    let n_rows = total_row_count;
    let n_cols = columns.len();

    if n_rows == 1 && n_cols == 1 {
        return ResultShape::Scalar;
    }
    if n_cols == 1 {
        return ResultShape::Series;
    }
    ResultShape::Table {
        columns: columns.to_vec(),
    }
}
