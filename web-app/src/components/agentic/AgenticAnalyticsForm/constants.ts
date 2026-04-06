export const THINKING_OPTIONS = [
  { value: "disabled", label: "Disabled" },
  { value: "enabled", label: "Enabled" },
  { value: "adaptive", label: "Adaptive" },
  { value: "effort:low", label: "Effort: Low" },
  { value: "effort:medium", label: "Effort: Medium" },
  { value: "effort:high", label: "Effort: High" }
] as const;

export const VENDOR_OPTIONS = [
  { value: "anthropic", label: "Anthropic" },
  { value: "openai", label: "OpenAI" },
  { value: "openai_compat", label: "OpenAI-compatible (Ollama, vLLM, …)" }
] as const;

export const SEMANTIC_ENGINE_VENDORS = [
  { value: "cube", label: "Cube" },
  { value: "looker", label: "Looker" }
] as const;

export const VALIDATION_RULE_NAMES = [
  { value: "metric_resolves", label: "Metric Resolves" },
  { value: "join_key_exists", label: "Join Key Exists" },
  { value: "filter_unambiguous", label: "Filter Unambiguous" },
  { value: "sql_syntax", label: "SQL Syntax" },
  { value: "tables_exist_in_catalog", label: "Tables Exist in Catalog" },
  { value: "spec_tables_present", label: "Spec Tables Present" },
  { value: "column_refs_valid", label: "Column Refs Valid" },
  { value: "non_empty", label: "Non Empty" },
  { value: "shape_match", label: "Shape Match" },
  { value: "no_nan_inf", label: "No NaN/Inf" },
  { value: "outlier_detection", label: "Outlier Detection" },
  { value: "timeseries_date_check", label: "Timeseries Date Check" }
] as const;

export const SQL_DIALECT_OPTIONS = [
  { value: "generic", label: "Generic" },
  { value: "ansi", label: "ANSI" },
  { value: "postgresql", label: "PostgreSQL" },
  { value: "mysql", label: "MySQL" },
  { value: "bigquery", label: "BigQuery" },
  { value: "duckdb", label: "DuckDB" },
  { value: "snowflake", label: "Snowflake" }
] as const;

export const STATE_NAMES = [
  "clarifying",
  "specifying",
  "solving",
  "executing",
  "interpreting",
  "diagnosing"
] as const;
