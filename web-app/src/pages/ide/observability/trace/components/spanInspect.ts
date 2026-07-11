import type { TimelineSpan } from "@/services/api/traces";

/** First present, parseable attribute value across a list of candidate keys. */
function firstAttr(attrs: Record<string, string>, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = attrs[key];
    if (value !== undefined && value !== "") return value;
  }
  return undefined;
}

function firstInt(attrs: Record<string, string>, keys: string[]): number | undefined {
  const raw = firstAttr(attrs, keys);
  if (raw === undefined) return undefined;
  const parsed = Number.parseInt(raw, 10);
  return Number.isNaN(parsed) ? undefined : parsed;
}

export function getSpanModel(span: TimelineSpan): string | undefined {
  return firstAttr(span.attributes, [
    "gen_ai.request.model",
    "gen_ai.response.model",
    "llm.model",
    "oxy.model"
  ]);
}

export interface SpanTokens {
  input?: number;
  output?: number;
  total: number;
}

export function getSpanTokens(span: TimelineSpan): SpanTokens | undefined {
  const input = firstInt(span.attributes, [
    "gen_ai.usage.input_tokens",
    "gen_ai.usage.prompt_tokens",
    "llm.token.prompt",
    "llm.usage.prompt_tokens"
  ]);
  const output = firstInt(span.attributes, [
    "gen_ai.usage.output_tokens",
    "gen_ai.usage.completion_tokens",
    "llm.token.completion",
    "llm.usage.completion_tokens"
  ]);
  const explicitTotal = firstInt(span.attributes, [
    "gen_ai.usage.total_tokens",
    "llm.token.total",
    "llm.usage.total_tokens"
  ]);
  const total = explicitTotal ?? (input ?? 0) + (output ?? 0);
  if (total === 0 && input === undefined && output === undefined) return undefined;
  return { input, output, total };
}

export function getSpanRows(span: TimelineSpan): number | undefined {
  return firstInt(span.attributes, ["db.rows", "db.row_count", "oxy.rows"]);
}

export function getSpanError(span: TimelineSpan): string | undefined {
  return firstAttr(span.attributes, ["error.type", "error.message", "exception.type"]);
}

/**
 * Compiled SQL a span ran, if any — checked across common OTel/DB attribute
 * keys and, failing that, span events carrying a statement. This is the
 * trace↔SQL correlation the inspector surfaces inline.
 */
export function extractSpanSql(span: TimelineSpan): string | undefined {
  const fromAttrs = firstAttr(span.attributes, [
    "db.statement",
    "db.statement.text",
    "db.query.text",
    "sql",
    "oxy.sql",
    "oxy.compiled_sql"
  ]);
  if (fromAttrs) return fromAttrs;

  for (const event of span.events) {
    const fromEvent = firstAttr(event.attributes, ["db.statement", "sql", "query", "statement"]);
    if (fromEvent) return fromEvent;
  }
  return undefined;
}

/**
 * Whether an attribute looks secret and should be redacted in the inspector.
 * Redacts credential-ish keys, and user/email-ish values.
 */
export function isSecretAttribute(key: string, value: string): boolean {
  const k = key.toLowerCase();
  if (/(password|secret|api[_-]?key|access[_-]?token|authorization|bearer)/.test(k)) return true;
  if (/token/.test(k) && /(key|@|bearer|[A-Za-z0-9_-]{20,})/.test(value)) return true;
  if (/(user|email)/.test(k) && value.includes("@")) return true;
  return false;
}
