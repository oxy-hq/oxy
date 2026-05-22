/// Substitute positional parameter placeholders (`$1`, `$2`, ... and `@p0`,
/// `@p1`, ...) or `?` placeholders with escaped string literals.
///
/// Airlayer returns parameterised SQL but connectors send raw SQL with no
/// separate parameter binding support.
pub fn substitute_params(sql: &str, params: &[String]) -> String {
    if params.is_empty() {
        return sql.to_string();
    }

    let uses_positional = (0..params.len())
        .any(|i| sql.contains(&format!("${}", i + 1)) || sql.contains(&format!("@p{}", i)));

    let mut result = sql.to_string();

    if uses_positional {
        // Replace $1, $2, ... and @p0, @p1, ... (right-to-left to avoid prefix
        // collision, e.g. $1 inside $10).
        for (i, param) in params.iter().enumerate().rev() {
            let escaped = param.replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replace(&format!("${}", i + 1), &literal);
            result = result.replace(&format!("@p{}", i), &literal);
        }
    } else {
        // Replace ? placeholders left-to-right (MySQL/Snowflake/SQLite).
        let mut param_index = 0;
        while result.contains('?') && param_index < params.len() {
            let escaped = params[param_index].replace('\'', "''");
            let literal = format!("'{}'", escaped);
            result = result.replacen('?', &literal, 1);
            param_index += 1;
        }
    }

    result
}
