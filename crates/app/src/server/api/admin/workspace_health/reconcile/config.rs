//! Strict-typed `reconcile.yml`. Stored as JSONB in `reconcile_configs`;
//! the runtime round-trips it back with `serde_json::from_value`.

use airlayer::engine::query::QueryRequest;
use serde::{Deserialize, Serialize};

use super::Tolerance;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileConfig {
    /// IANA timezone every check window resolves in (e.g. `America/Los_Angeles`).
    /// Absent = UTC. Overridable per check via `window.timezone`.
    #[serde(
        default,
        deserialize_with = "de_timezone",
        skip_serializing_if = "Option::is_none"
    )]
    pub timezone: Option<String>,
    /// Freshness watermark: resolve every window as of `now - freshness`, so a
    /// warehouse that is days behind on ingestion still compares a period that
    /// has actually landed. Absent = zero. Overridable via `window.freshness`.
    #[serde(
        default,
        with = "oxy_metric_monitoring::config::duration_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub freshness: Option<std::time::Duration>,
    #[serde(default)]
    pub checks: Vec<ReconcileCheck>,
}

impl ReconcileConfig {
    /// Push file-level `timezone` / `freshness` into any window that did not set
    /// them, so each [`Window`] is self-contained for `resolve_window`.
    pub fn apply_defaults(&mut self) {
        // Bind first: iterating `&mut self.checks` while reading `self.*` would
        // be a double borrow.
        let (tz, freshness) = (self.timezone.clone(), self.freshness);
        for c in &mut self.checks {
            if c.window.timezone.is_none() {
                c.window.timezone = tz.clone();
            }
            if c.window.freshness.is_none() {
                c.window.freshness = freshness;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileCheck {
    /// Stable machine identifier.
    pub name: String,
    /// Friendly UI text (check-level). Optional, purely presentational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Shared comparison window (applies to BOTH operands).
    pub window: Window,
    pub tolerance: Tolerance,
    /// Optional per-segment fan-out dimension. Parsed today; fan-out execution
    /// is a follow-up (currently unused by the runner).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// The value being checked.
    pub actual: Operand,
    /// The reference value `actual` is compared against (`pct` denominator).
    pub expected: Operand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub last: u32,
    pub grain: Grain,
    /// Number of grains to shift back, so the current incomplete period is
    /// excluded (offset: 1, grain: day == "yesterday").
    #[serde(default)]
    pub offset: u32,
    /// Freshness watermark: resolve this window as of `now - freshness` rather
    /// than `now`. Absent = the file-level `freshness`, else zero.
    #[serde(
        default,
        with = "oxy_metric_monitoring::config::duration_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub freshness: Option<std::time::Duration>,
    /// IANA timezone this window's calendar snapping happens in. Absent = the
    /// file-level `timezone`, else UTC.
    ///
    /// Set this ONLY when the check's `time_dimension` is a timestamp. On a
    /// DATE / business-date column the warehouse has already assigned the local
    /// date, and converting again shifts the comparison by the UTC offset.
    #[serde(
        default,
        deserialize_with = "de_timezone",
        skip_serializing_if = "Option::is_none"
    )]
    pub timezone: Option<String>,
    /// Which weekday a `grain: week` window starts on. Ignored for day/month.
    /// Defaults to Sunday to match the Command Center's weekly ribbon and
    /// 12-week charts — that reporting convention is the only reason, *not* a
    /// warehouse default: most dialects truncate weeks to Monday (see the
    /// dialect table on `oxy_metric_monitoring::config::WeekStart`, which
    /// defaults to Monday for exactly that reason).
    ///
    /// Unlike the monitor knob, this one picks the **actual comparison window**
    /// rather than just trimming an incomplete period, so a Mon–Sun business
    /// silently reconciles Sun–Sat until it sets `week_start: monday`.
    #[serde(default)]
    pub week_start: WeekStart,
}

impl Window {
    /// Resolved timezone. Defaults to UTC, which reproduces the pre-2026-07
    /// UTC-only behavior exactly. Infallible — the name was validated at parse
    /// time by [`de_timezone`], so `resolve_window` can stay infallible too.
    pub fn effective_timezone(&self) -> chrono_tz::Tz {
        self.timezone
            .as_deref()
            .and_then(|s| s.parse::<chrono_tz::Tz>().ok())
            .unwrap_or(chrono_tz::UTC)
    }

    /// Resolved freshness watermark. Defaults to zero, which reproduces the
    /// pre-freshness behavior exactly.
    pub fn effective_freshness(&self) -> chrono::Duration {
        self.freshness
            .and_then(|d| chrono::Duration::from_std(d).ok())
            .unwrap_or_else(chrono::Duration::zero)
    }
}

/// Validate an IANA timezone name while deserializing.
///
/// Validation belongs here rather than in a post-parse pass so the error carries
/// the right serde path and is naturally a `serde_json::Error`. `resolve_window`
/// is infallible by design and has nowhere to report a bad name.
fn de_timezone<'de, D>(d: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let Some(s) = Option::<String>::deserialize(d)? else {
        return Ok(None);
    };
    s.parse::<chrono_tz::Tz>()
        .map_err(|_| serde::de::Error::custom(format!("unknown IANA timezone '{s}'")))?;
    Ok(Some(s))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    Day,
    Week,
    Month,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeekStart {
    #[default]
    Sunday,
    Monday,
}

/// One side of a check: an optional friendly `label` plus exactly one kind
/// block (`semantic` / `sql` / `external` / `constant`).
///
/// A per-kind `Option` struct (not an externally-tagged enum) so `label` can
/// sit as a sibling of the kind — the shipped "exactly one mode" pattern.
/// [`Operand::resolve_kind`] validates and resolves the populated kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operand {
    /// Friendly label for UI rendering; defaults by side ("Actual"/"Expected").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// KIND: full airlayer semantic query bound to a `time_dimension`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic: Option<SemanticSpec>,
    /// KIND: raw SQL run against a named `database`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<SqlSpec>,
    /// KIND: an authoritative external source (Toast first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalSpec>,
    /// KIND: a fixed number (e.g. assert a count equals 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant: Option<f64>,
}

/// The resolved kind, borrowed from the [`Operand`].
#[derive(Debug, Clone)]
pub enum OperandKind<'a> {
    Semantic(&'a SemanticSpec),
    Sql(&'a SqlSpec),
    External(&'a ExternalSpec),
    Constant(f64),
}

impl Operand {
    /// The label for UI, defaulting by side ("Actual" / "Expected").
    pub fn label_or(&self, default: &str) -> String {
        self.label.clone().unwrap_or_else(|| default.to_string())
    }

    /// Validate exactly one kind is set (and, for semantic, exactly one
    /// measure); return the resolved kind. Returns a human-readable error the
    /// runner surfaces as a degraded verdict.
    pub fn resolve_kind(&self) -> Result<OperandKind<'_>, String> {
        let set = [
            self.semantic.is_some(),
            self.sql.is_some(),
            self.external.is_some(),
            self.constant.is_some(),
        ]
        .into_iter()
        .filter(|b| *b)
        .count();
        match set {
            0 => {
                return Err(
                    "reconcile operand: specify one of semantic/sql/external/constant".to_string(),
                );
            }
            1 => {}
            _ => return Err("reconcile operand: specify exactly one kind".to_string()),
        }

        if let Some(semantic) = &self.semantic {
            if semantic.query.measures.len() != 1 {
                return Err(
                    "reconcile operand: semantic query must specify exactly one measure"
                        .to_string(),
                );
            }
            return Ok(OperandKind::Semantic(semantic));
        }
        if let Some(sql) = &self.sql {
            return Ok(OperandKind::Sql(sql));
        }
        if let Some(external) = &self.external {
            return Ok(OperandKind::External(external));
        }
        // Exactly one is set and it wasn't the three above ⇒ constant.
        Ok(OperandKind::Constant(
            self.constant.expect("constant is the remaining set kind"),
        ))
    }
}

/// A full airlayer semantic query bound to a time dimension (the window binds
/// here). Exactly one measure (validated by [`Operand::resolve_kind`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSpec {
    /// Full airlayer query params (measures/dimensions/filters/segments/…).
    #[serde(flatten)]
    pub query: QueryRequest,
    /// The dimension the shared window binds to (required).
    pub time_dimension: String,
}

/// Raw SQL run against a named workspace connection. `{{ start_date }}` /
/// `{{ end_date }}` are bound from the shared window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SqlSpec {
    /// The `config.yml` connection name the `sql` runs against.
    pub database: String,
    pub sql: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExternalSpec {
    /// Adapter registry key — the external system *kind*, e.g. "toast".
    pub source: String,
    /// Named `config.yml` integration backing this operand; first of `source`
    /// kind when omitted. Supplies the source's secret var-names and API base URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
    /// External metric field to read from each report row (e.g. "net_sales").
    pub metric: String,
    /// Which restaurant GUIDs to sum on the external side. Absent/empty sums
    /// EVERY restaurant in the report (all-restaurants aggregate); present sums
    /// only the listed GUIDs (per-restaurant). Mirror with a semantic operand's
    /// filters so both sides scope to the same restaurants.
    #[serde(default)]
    pub restaurants: Vec<String>,
}

pub fn parse_reconcile_config(v: &serde_json::Value) -> Result<ReconcileConfig, serde_json::Error> {
    let mut cfg: ReconcileConfig = serde_json::from_value(v.clone())?;
    cfg.apply_defaults();
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operand(yaml: &str) -> Operand {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn semantic_operand_resolves_single_measure() {
        let op = operand(
            r#"
label: Oxy net sales
semantic:
  measures: [sales.net]
  time_dimension: sales.business_date
  segments: [sales.dine_in]
  filters:
    - member: sales.restaurant_id
      operator: equals
      values: ["a", "b"]
"#,
        );
        assert_eq!(op.label_or("Actual"), "Oxy net sales");
        match op.resolve_kind().unwrap() {
            OperandKind::Semantic(s) => {
                assert_eq!(s.query.measures, vec!["sales.net".to_string()]);
                assert_eq!(s.query.segments, vec!["sales.dine_in".to_string()]);
                assert_eq!(s.query.filters.len(), 1);
                assert_eq!(s.time_dimension, "sales.business_date");
            }
            other => panic!("expected Semantic, got {other:?}"),
        }
    }

    #[test]
    fn sql_operand_resolves() {
        let op = operand(
            r#"
sql:
  database: main
  sql: "select sum(v) from t where d between '{{ start_date }}' and '{{ end_date }}'"
"#,
        );
        match op.resolve_kind().unwrap() {
            OperandKind::Sql(s) => {
                assert_eq!(s.database, "main");
                assert!(s.sql.contains("start_date"));
            }
            other => panic!("expected Sql, got {other:?}"),
        }
    }

    #[test]
    fn external_operand_resolves() {
        let op = operand(
            r#"
label: Toast net sales
external:
  source: toast
  integration: toast_main
  metric: net_sales
  restaurants: ["abc123"]
"#,
        );
        match op.resolve_kind().unwrap() {
            OperandKind::External(e) => {
                assert_eq!(e.source, "toast");
                assert_eq!(e.integration.as_deref(), Some("toast_main"));
                assert_eq!(e.metric, "net_sales");
                assert_eq!(e.restaurants, vec!["abc123".to_string()]);
            }
            other => panic!("expected External, got {other:?}"),
        }
    }

    #[test]
    fn constant_operand_resolves_including_zero() {
        let op = operand("constant: 0\n");
        assert!(matches!(
            op.resolve_kind().unwrap(),
            OperandKind::Constant(n) if n == 0.0
        ));
        // No label → defaults by side.
        assert_eq!(op.label_or("Expected"), "Expected");
    }

    #[test]
    fn zero_kinds_errors() {
        let op = operand("label: Nothing\n");
        assert!(
            op.resolve_kind()
                .unwrap_err()
                .contains("one of semantic/sql/external/constant")
        );
    }

    #[test]
    fn two_kinds_errors() {
        let op = operand("constant: 0\nexternal: { source: toast, metric: x }\n");
        assert!(op.resolve_kind().unwrap_err().contains("exactly one kind"));
    }

    #[test]
    fn semantic_more_than_one_measure_errors() {
        let op = operand("semantic: { measures: [m.x, m.y], time_dimension: m.d }\n");
        assert!(
            op.resolve_kind()
                .unwrap_err()
                .contains("exactly one measure")
        );
    }

    #[test]
    fn label_or_defaults_and_overrides() {
        let bare = operand("constant: 1\n");
        assert_eq!(bare.label_or("Actual"), "Actual");
        assert_eq!(bare.label_or("Expected"), "Expected");
        let labeled = operand("label: Custom\nconstant: 1\n");
        assert_eq!(labeled.label_or("Actual"), "Custom");
    }

    #[test]
    fn parses_a_full_semantic_vs_external_check() {
        let yaml = r#"
checks:
  - name: revenue_vs_toast
    description: Daily net sales reconciled against Toast POS totals.
    window: { last: 1, grain: day, offset: 1 }
    tolerance: { abs: 1.0, pct: 0.5, combinator: and }
    actual:
      label: Oxy net sales
      semantic:
        measures: [orders.net_sales]
        time_dimension: orders.created_date
        filters:
          - member: orders.restaurant_id
            operator: equals
            values: ["abc123"]
    expected:
      label: Toast net sales
      external:
        source: toast
        integration: toast_main
        metric: net_sales
        restaurants: ["abc123"]
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        let c = &cfg.checks[0];
        assert_eq!(c.name, "revenue_vs_toast");
        assert_eq!(
            c.description.as_deref(),
            Some("Daily net sales reconciled against Toast POS totals.")
        );
        assert_eq!(c.window.offset, 1);
        assert_eq!(c.actual.label_or("Actual"), "Oxy net sales");
        assert_eq!(c.expected.label_or("Expected"), "Toast net sales");
        assert!(matches!(
            c.actual.resolve_kind().unwrap(),
            OperandKind::Semantic(_)
        ));
        match c.expected.resolve_kind().unwrap() {
            OperandKind::External(e) => {
                assert_eq!(e.metric, "net_sales");
                assert_eq!(e.restaurants, vec!["abc123".to_string()]);
            }
            other => panic!("expected External, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_constant_expected_check() {
        let yaml = r#"
checks:
  - name: no_orphan_rows
    window: { last: 1, grain: day, offset: 1 }
    tolerance: { abs: 0, pct: 0, combinator: or }
    actual:
      label: Orphan rows
      sql:
        database: main
        sql: "select count(*) from orphans where d = '{{ start_date }}'"
    expected:
      constant: 0
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        let c = &cfg.checks[0];
        assert_eq!(c.description, None);
        assert!(matches!(
            c.actual.resolve_kind().unwrap(),
            OperandKind::Sql(_)
        ));
        assert!(matches!(
            c.expected.resolve_kind().unwrap(),
            OperandKind::Constant(n) if n == 0.0
        ));
        // No label on the constant → defaults to "Expected".
        assert_eq!(c.expected.label_or("Expected"), "Expected");
    }

    #[test]
    fn window_freshness_parses_humantime_units() {
        let w: Window = serde_yaml::from_str("last: 1\ngrain: day\nfreshness: 3d\n").unwrap();
        assert_eq!(w.effective_freshness(), chrono::Duration::days(3));

        let w: Window = serde_yaml::from_str("last: 1\ngrain: day\nfreshness: 30m\n").unwrap();
        assert_eq!(w.effective_freshness(), chrono::Duration::minutes(30));

        let w: Window = serde_yaml::from_str("last: 1\ngrain: day\nfreshness: 12h\n").unwrap();
        assert_eq!(w.effective_freshness(), chrono::Duration::hours(12));
    }

    #[test]
    fn absent_window_fields_mean_utc_and_zero() {
        // The deployed shape: neither field present.
        let w: Window = serde_yaml::from_str("last: 1\ngrain: day\noffset: 1\n").unwrap();
        assert_eq!(w.effective_timezone(), chrono_tz::UTC);
        assert_eq!(w.effective_freshness(), chrono::Duration::zero());
    }

    #[test]
    fn file_level_defaults_fill_unset_windows() {
        let yaml = r#"
timezone: America/Los_Angeles
freshness: 3d
checks:
  - name: inherits
    window: { last: 1, grain: day, offset: 1 }
    tolerance: { abs: 1.0, pct: 0.5 }
    actual: { constant: 1 }
    expected: { constant: 1 }
  - name: overrides
    window: { last: 1, grain: day, offset: 1, timezone: Europe/Berlin, freshness: 30m }
    tolerance: { abs: 1.0, pct: 0.5 }
    actual: { constant: 1 }
    expected: { constant: 1 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();

        let inherits = &cfg.checks[0].window;
        assert_eq!(
            inherits.effective_timezone(),
            chrono_tz::America::Los_Angeles
        );
        assert_eq!(inherits.effective_freshness(), chrono::Duration::days(3));

        let overrides = &cfg.checks[1].window;
        assert_eq!(overrides.effective_timezone(), chrono_tz::Europe::Berlin);
        assert_eq!(
            overrides.effective_freshness(),
            chrono::Duration::minutes(30)
        );
    }

    #[test]
    fn invalid_timezone_is_rejected_at_parse() {
        let yaml = r#"
checks:
  - name: bad_tz
    window: { last: 1, grain: day, offset: 1, timezone: Mars/Olympus_Mons }
    tolerance: { abs: 1.0, pct: 0.5 }
    actual: { constant: 1 }
    expected: { constant: 1 }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let err = parse_reconcile_config(&v).unwrap_err().to_string();
        assert!(
            err.contains("Mars/Olympus_Mons"),
            "error should name the offending value, got: {err}"
        );
    }

    #[test]
    fn invalid_file_level_timezone_is_rejected_at_parse() {
        let yaml = "timezone: Not/AZone\nchecks: []\n";
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let err = parse_reconcile_config(&v).unwrap_err().to_string();
        assert!(
            err.contains("Not/AZone"),
            "error should name the offending value, got: {err}"
        );
    }

    #[test]
    fn parses_group_by_and_defaults_combinator() {
        let yaml = r#"
checks:
  - name: c
    window: { last: 1, grain: day, offset: 1 }
    tolerance: { abs: 1.0, pct: 0.5 }
    group_by: sales.restaurant_id
    actual: { semantic: { measures: [m.x], time_dimension: m.d } }
    expected: { external: { source: toast, metric: x } }
"#;
        let v: serde_json::Value = serde_yaml::from_str(yaml).unwrap();
        let cfg = parse_reconcile_config(&v).unwrap();
        let c = &cfg.checks[0];
        assert_eq!(c.group_by.as_deref(), Some("sales.restaurant_id"));
        assert!(matches!(
            c.tolerance.combinator,
            super::super::compare::Combinator::And
        ));
    }
}
