//! The outer row ceiling every semantic answer is read through.
//!
//! One definition for the `/semantic-query` endpoint and the function host's
//! rollup read (`custom_apps_functions::host::read_rollup`): "a function sees one
//! row ceiling however the question was answered" is only true while the two are
//! the same code.

/// Wrap compiled SQL in `SELECT * FROM (...) LIMIT N`. Same pattern
/// `query.rs` uses — keeps the row cap consistent across endpoints
/// without forcing airlayer to know about it.
///
/// `pub(crate)` because a bundle's `ctx.semantic` reads its rollup through the
/// identical wrap (`custom_apps_functions::host::read_rollup`), and "a function
/// sees one row ceiling however the question was answered" is only true while
/// the two are the same code.
pub fn wrap_with_limit(sql: &str, max_rows: usize) -> String {
    let sql_trimmed = sql.trim_end().trim_end_matches(';').trim_end();
    format!("SELECT * FROM (\n{sql_trimmed}\n) AS oxy_semantic_query LIMIT {max_rows}")
}
