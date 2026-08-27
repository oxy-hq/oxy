//! JSON ↔ Postgres value conversion for the transaction path.
//!
//! Two directions, both deliberately **fail-loud**:
//!
//! - **Params in.** We `prepare()` the statement first and convert each JSON
//!   argument to the type Postgres actually inferred for that placeholder,
//!   rather than guessing from the JSON shape. That is what makes `$1` work
//!   against an `int4` column when JSON only has one number type — guessing
//!   `i64` would send `INT8` and lean on an implicit cast that does not exist
//!   in every context.
//! - **Rows out.** Decoded by the column's real OID.
//!
//! A type we cannot represent is an error naming the column/position and the
//! Postgres type, never a silent coercion. Silently turning a `numeric` into an
//! `f64` is how money columns lose cents, and this path exists specifically for
//! the workloads where that matters.

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use tokio_postgres::Row;
use tokio_postgres::types::{ToSql, Type};
use uuid::Uuid;

use crate::connector::ConnectorError;

/// An owned, type-erased bound parameter.
///
/// `tokio_postgres` wants `&(dyn ToSql + Sync)`, so the owned values have to
/// outlive the call; this box is what they live in.
pub(super) type OwnedParam = Box<dyn ToSql + Sync + Send>;

/// Convert one JSON argument to the Postgres type inferred for its placeholder.
///
/// `position` is 1-based to match `$1` in the author's SQL — an error that says
/// "$2" when the author wrote `$2` costs nothing and saves a round of guessing.
pub(super) fn json_to_param(
    value: &serde_json::Value,
    expected: &Type,
    position: usize,
) -> Result<OwnedParam, ConnectorError> {
    use serde_json::Value as J;

    // NULL is representable in every type, and `Option<T>::None` carries the
    // type through, so match on the expected type rather than the JSON shape.
    if value.is_null() {
        return null_param(expected, position);
    }

    match (expected, value) {
        (&Type::BOOL, J::Bool(b)) => Ok(Box::new(*b)),
        (&Type::INT2, J::Number(n)) => int_param(n, position, |i| {
            i16::try_from(i).ok().map(|v| Box::new(v) as OwnedParam)
        }),
        (&Type::INT4, J::Number(n)) => int_param(n, position, |i| {
            i32::try_from(i).ok().map(|v| Box::new(v) as OwnedParam)
        }),
        (&Type::INT8, J::Number(n)) => int_param(n, position, |i| Some(Box::new(i) as OwnedParam)),
        (&Type::FLOAT4, J::Number(n)) => n
            .as_f64()
            .map(|f| Box::new(f as f32) as OwnedParam)
            .ok_or_else(|| bad_param(position, "float4", "a JSON number")),
        (&Type::FLOAT8, J::Number(n)) => n
            .as_f64()
            .map(|f| Box::new(f) as OwnedParam)
            .ok_or_else(|| bad_param(position, "float8", "a JSON number")),
        (&Type::TEXT | &Type::VARCHAR | &Type::BPCHAR | &Type::NAME, J::String(s)) => {
            Ok(Box::new(s.clone()))
        }
        (&Type::JSON | &Type::JSONB, v) => Ok(Box::new(v.clone())),
        // Temporal and UUID params arrive as strings — JSON has no native
        // representation for either, and every JS date serialiser produces
        // ISO-8601, so that is what we parse.
        (&Type::UUID, J::String(s)) => Uuid::parse_str(s)
            .map(|u| Box::new(u) as OwnedParam)
            .map_err(|e| bad_param(position, "uuid", &format!("a UUID string ({e})"))),
        (&Type::TIMESTAMPTZ, J::String(s)) => DateTime::parse_from_rfc3339(s)
            .map(|d| Box::new(d.with_timezone(&Utc)) as OwnedParam)
            .map_err(|e| {
                bad_param(
                    position,
                    "timestamptz",
                    &format!("an RFC-3339 timestamp ({e})"),
                )
            }),
        (&Type::TIMESTAMP, J::String(s)) => parse_naive_datetime(s)
            .map(|d| Box::new(d) as OwnedParam)
            .ok_or_else(|| {
                bad_param(
                    position,
                    "timestamp",
                    "an ISO-8601 timestamp without an offset",
                )
            }),
        (&Type::DATE, J::String(s)) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(|d| Box::new(d) as OwnedParam)
            .map_err(|e| bad_param(position, "date", &format!("a YYYY-MM-DD date ({e})"))),
        (&Type::TIME, J::String(s)) => NaiveTime::parse_from_str(s, "%H:%M:%S%.f")
            .map(|t| Box::new(t) as OwnedParam)
            .map_err(|e| bad_param(position, "time", &format!("an HH:MM:SS time ({e})"))),
        // An untyped placeholder (`$1` with nothing to infer from) arrives as
        // `text`-ish UNKNOWN; sending the JSON string is the useful reading.
        (&Type::UNKNOWN, J::String(s)) => Ok(Box::new(s.clone())),
        _ => Err(unsupported_param(position, expected, value)),
    }
}

fn null_param(expected: &Type, position: usize) -> Result<OwnedParam, ConnectorError> {
    match *expected {
        Type::BOOL => Ok(Box::new(None::<bool>)),
        Type::INT2 => Ok(Box::new(None::<i16>)),
        Type::INT4 => Ok(Box::new(None::<i32>)),
        Type::INT8 => Ok(Box::new(None::<i64>)),
        Type::FLOAT4 => Ok(Box::new(None::<f32>)),
        Type::FLOAT8 => Ok(Box::new(None::<f64>)),
        Type::JSON | Type::JSONB => Ok(Box::new(None::<serde_json::Value>)),
        Type::UUID => Ok(Box::new(None::<Uuid>)),
        Type::TIMESTAMPTZ => Ok(Box::new(None::<DateTime<Utc>>)),
        Type::TIMESTAMP => Ok(Box::new(None::<NaiveDateTime>)),
        Type::DATE => Ok(Box::new(None::<NaiveDate>)),
        Type::TIME => Ok(Box::new(None::<NaiveTime>)),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN => {
            Ok(Box::new(None::<String>))
        }
        ref other => Err(unsupported_param(position, other, &serde_json::Value::Null)),
    }
}

fn int_param(
    n: &serde_json::Number,
    position: usize,
    fit: impl Fn(i64) -> Option<OwnedParam>,
) -> Result<OwnedParam, ConnectorError> {
    let i = n
        .as_i64()
        .ok_or_else(|| bad_param(position, "an integer", "a whole JSON number"))?;
    fit(i).ok_or_else(|| {
        ConnectorError::Other(format!(
            "parameter ${position} value {i} is out of range for the column's integer type"
        ))
    })
}

fn bad_param(position: usize, pg_type: &str, wanted: &str) -> ConnectorError {
    ConnectorError::Other(format!(
        "parameter ${position} maps to Postgres `{pg_type}`, which needs {wanted}"
    ))
}

/// The escape hatch matters more than the diagnosis here: `$1::text::<type>`
/// forces the placeholder to be inferred as `text`, which we can always send,
/// and lets Postgres do the final cast itself.
fn unsupported_param(
    position: usize,
    expected: &Type,
    value: &serde_json::Value,
) -> ConnectorError {
    let shape = json_shape(value);
    ConnectorError::Other(format!(
        "cannot bind {shape} to parameter ${position}, which Postgres inferred as \
         `{expected}`. Bind it as text and let Postgres cast — write `${position}::text::{expected}` \
         in the SQL and pass a string."
    ))
}

/// Accept both the `T` separator JS emits and the space Postgres prints, with
/// or without fractional seconds — the four shapes an author realistically has
/// in hand for a `timestamp` column.
fn parse_naive_datetime(s: &str) -> Option<NaiveDateTime> {
    // A trailing `Z` is a lie on a `timestamp` (no offset) column; rejecting it
    // would be pedantic when the intent is unambiguous, so strip it.
    let s = s.strip_suffix('Z').unwrap_or(s);
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    None
}

fn json_shape(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Reject a result shape we cannot decode **before** any row is read.
///
/// `prepare` already told us every output column's type, so an undecodable
/// column can be caught while the statement is still unstarted. That matters
/// for more than tidiness: discovering it mid-stream means abandoning a running
/// query, which ends the transaction — and the error we raise names a fix
/// (`amount::text`) the author is meant to apply and retry. Caught here, the
/// block is untouched and that retry works in the same transaction, exactly as
/// the docs promise.
pub(super) fn check_decodable(columns: &[tokio_postgres::Column]) -> Result<(), ConnectorError> {
    for col in columns {
        if !is_decodable(col.type_()) {
            return Err(unsupported_column(col.name(), col.type_()));
        }
    }
    Ok(())
}

/// Types [`cell_to_json`] can render. Kept adjacent to it — the two must agree,
/// and a type added there without adding it here silently reintroduces the
/// mid-stream failure this exists to prevent.
fn is_decodable(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::BOOL
            | Type::INT2
            | Type::INT4
            | Type::INT8
            | Type::FLOAT4
            | Type::FLOAT8
            | Type::TEXT
            | Type::VARCHAR
            | Type::BPCHAR
            | Type::NAME
            | Type::UNKNOWN
            | Type::JSON
            | Type::JSONB
            | Type::UUID
            | Type::TIMESTAMPTZ
            | Type::TIMESTAMP
            | Type::DATE
            | Type::TIME
    )
}

/// Decode a returned row into a JSON object keyed by column name.
pub(super) fn row_to_json(row: &Row) -> Result<serde_json::Value, ConnectorError> {
    let mut obj = serde_json::Map::with_capacity(row.columns().len());
    for (idx, col) in row.columns().iter().enumerate() {
        obj.insert(
            col.name().to_string(),
            cell_to_json(row, idx, col.type_(), col.name())?,
        );
    }
    Ok(serde_json::Value::Object(obj))
}

fn cell_to_json(
    row: &Row,
    idx: usize,
    ty: &Type,
    name: &str,
) -> Result<serde_json::Value, ConnectorError> {
    use serde_json::Value as J;

    // `try_get::<Option<T>>` collapses NULL into `None` for every arm, so NULL
    // handling does not need a branch per type.
    let v = match *ty {
        Type::BOOL => opt(row.try_get::<_, Option<bool>>(idx), name, ty)?.map(J::Bool),
        Type::INT2 => opt(row.try_get::<_, Option<i16>>(idx), name, ty)?.map(J::from),
        Type::INT4 => opt(row.try_get::<_, Option<i32>>(idx), name, ty)?.map(J::from),
        Type::INT8 => opt(row.try_get::<_, Option<i64>>(idx), name, ty)?.map(J::from),
        Type::FLOAT4 => opt(row.try_get::<_, Option<f32>>(idx), name, ty)?.map(J::from),
        Type::FLOAT8 => opt(row.try_get::<_, Option<f64>>(idx), name, ty)?.map(J::from),
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::UNKNOWN => {
            opt(row.try_get::<_, Option<String>>(idx), name, ty)?.map(J::String)
        }
        Type::JSON | Type::JSONB => opt(row.try_get::<_, Option<J>>(idx), name, ty)?,
        // Rendered as strings, in the format each type's param arm parses — so
        // a value read here can be passed straight back as a parameter without
        // the author reformatting it.
        Type::UUID => {
            opt(row.try_get::<_, Option<Uuid>>(idx), name, ty)?.map(|u| J::String(u.to_string()))
        }
        Type::TIMESTAMPTZ => opt(row.try_get::<_, Option<DateTime<Utc>>>(idx), name, ty)?
            .map(|d| J::String(d.to_rfc3339_opts(chrono::SecondsFormat::Micros, true))),
        Type::TIMESTAMP => opt(row.try_get::<_, Option<NaiveDateTime>>(idx), name, ty)?
            .map(|d| J::String(d.format("%Y-%m-%dT%H:%M:%S%.6f").to_string())),
        Type::DATE => opt(row.try_get::<_, Option<NaiveDate>>(idx), name, ty)?
            .map(|d| J::String(d.format("%Y-%m-%d").to_string())),
        Type::TIME => opt(row.try_get::<_, Option<NaiveTime>>(idx), name, ty)?
            .map(|t| J::String(t.format("%H:%M:%S%.6f").to_string())),
        _ => return Err(unsupported_column(name, ty)),
    };
    Ok(v.unwrap_or(J::Null))
}

fn opt<T>(
    got: Result<Option<T>, tokio_postgres::Error>,
    name: &str,
    ty: &Type,
) -> Result<Option<T>, ConnectorError> {
    got.map_err(|e| {
        ConnectorError::Other(format!(
            "failed to decode column `{name}` (Postgres type `{ty}`): {e}"
        ))
    })
}

/// Same shape of advice as [`unsupported_param`], in the other direction:
/// cast in the SELECT list rather than asking us to grow a decoder.
fn unsupported_column(name: &str, ty: &Type) -> ConnectorError {
    ConnectorError::Other(format!(
        "result column `{name}` has Postgres type `{ty}`, which cannot be returned \
         directly. Cast it to text in the SELECT list and parse it in your function — cast the \
         whole expression, not the output name (`avg(qty)::text`, `amount::text`). Note \
         `avg`/`sum` over any numeric type, and a bare decimal literal, are all `numeric`. \
         (Supported as-is: bool, int2/4/8, float4/8, text/varchar/char/name, json/jsonb, \
         uuid, timestamptz/timestamp, date, time.)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn int4_placeholder_accepts_a_json_number() {
        assert!(json_to_param(&json!(42), &Type::INT4, 1).is_ok());
    }

    #[test]
    fn int4_placeholder_rejects_an_out_of_range_value() {
        let err = json_to_param(&json!(i64::MAX), &Type::INT4, 2)
            .unwrap_err()
            .to_string();
        assert!(err.contains("$2"), "names the placeholder: {err}");
        assert!(err.contains("out of range"), "says why: {err}");
    }

    #[test]
    fn null_is_representable_for_every_supported_type() {
        for ty in [Type::BOOL, Type::INT4, Type::INT8, Type::FLOAT8, Type::TEXT] {
            assert!(
                json_to_param(&serde_json::Value::Null, &ty, 1).is_ok(),
                "null should bind for {ty}"
            );
        }
    }

    #[test]
    fn is_decodable_agrees_with_the_types_cell_to_json_renders() {
        for ty in [
            Type::BOOL,
            Type::INT2,
            Type::INT4,
            Type::INT8,
            Type::FLOAT4,
            Type::FLOAT8,
            Type::TEXT,
            Type::VARCHAR,
            Type::BPCHAR,
            Type::NAME,
            Type::JSON,
            Type::JSONB,
            Type::UUID,
            Type::TIMESTAMPTZ,
            Type::TIMESTAMP,
            Type::DATE,
            Type::TIME,
        ] {
            assert!(is_decodable(&ty), "cell_to_json renders {ty}");
        }
        // The documented escape-hatch cases must stay *un*decodable, or the
        // pre-flight check stops firing and they fail mid-stream again.
        for ty in [Type::NUMERIC, Type::BYTEA, Type::INET] {
            assert!(!is_decodable(&ty), "{ty} has no cell_to_json arm");
        }
    }

    #[test]
    fn uuid_and_temporal_params_parse_from_strings() {
        assert!(
            json_to_param(
                &json!("018f8c1e-0000-7000-8000-000000000000"),
                &Type::UUID,
                1
            )
            .is_ok()
        );
        assert!(json_to_param(&json!("2026-08-18T12:00:00Z"), &Type::TIMESTAMPTZ, 1).is_ok());
        assert!(json_to_param(&json!("2026-08-18T12:00:00"), &Type::TIMESTAMP, 1).is_ok());
        assert!(json_to_param(&json!("2026-08-18"), &Type::DATE, 1).is_ok());
        assert!(json_to_param(&json!("12:00:00"), &Type::TIME, 1).is_ok());
    }

    #[test]
    fn a_malformed_uuid_names_the_type_rather_than_binding_garbage() {
        let err = json_to_param(&json!("not-a-uuid"), &Type::UUID, 3)
            .unwrap_err()
            .to_string();
        assert!(err.contains("$3") && err.contains("uuid"), "{err}");
    }

    #[test]
    fn naive_timestamps_accept_the_shapes_an_author_actually_has() {
        // JS `toISOString()` (with Z), Postgres's own printed form, and both
        // without fractional seconds.
        for s in [
            "2026-08-18T12:00:00.123456Z",
            "2026-08-18 12:00:00.123456",
            "2026-08-18T12:00:00",
            "2026-08-18 12:00:00",
        ] {
            assert!(parse_naive_datetime(s).is_some(), "should parse: {s}");
        }
        assert!(parse_naive_datetime("18/08/2026").is_none());
    }

    #[test]
    fn an_unsupported_type_points_at_the_text_cast_escape_hatch() {
        let err = json_to_param(&json!("12.50"), &Type::NUMERIC, 1)
            .unwrap_err()
            .to_string();
        assert!(err.contains("$1::text::numeric"), "gives the fix: {err}");
    }

    #[test]
    fn a_string_does_not_silently_become_a_number() {
        // The failure this guards: accepting "42" for an int4 column would make
        // a typo in app code succeed until the day the string is not numeric.
        assert!(json_to_param(&json!("42"), &Type::INT4, 1).is_err());
    }

    #[test]
    fn objects_and_arrays_bind_to_json_columns() {
        assert!(json_to_param(&json!({"a": 1}), &Type::JSONB, 1).is_ok());
        assert!(json_to_param(&json!([1, 2]), &Type::JSON, 1).is_ok());
    }
}
