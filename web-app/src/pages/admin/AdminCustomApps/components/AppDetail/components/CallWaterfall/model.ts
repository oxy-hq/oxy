import { ADMIN_TONE } from "@/pages/admin/components/adminTone";
import type { InvocationCall, InvocationCalls, InvocationCallsReason } from "@/types/apps";

/**
 * The pure half of the call waterfall: what each bar says, where it sits and
 * what colour it is. Everything here reads only the shape facets the server
 * records (verb, table, host, counts) — there is nothing else to read.
 */

/** The narrowest a bar is drawn, as a percentage of the track, so a 0 ms call
 *  (a gate refusal) is still a visible mark. */
export const MIN_BAR_PCT = 0.5;

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** `verb target`, either half optional; `null` when the call named neither. */
function verbTarget(call: InvocationCall): string | null {
  const text = [call.verb, call.target].filter(Boolean).join(" ");
  return text || null;
}

/** The rows facet of a query reply, when the reply said how many. A stream's
 *  reply may not, so it is optional for both. */
function rowsSuffix(call: InvocationCall): string {
  if (call.rows == null) return "";
  return ` · ${plural(call.rows, "row", "rows")}${call.rows_truncated ? " (truncated)" : ""}`;
}

/** What one call did, in a few words: `SELECT orders · 120 rows`,
 *  `POST api.toasttab.com · 200`, `UPDATE app_store.orders`, `3 recipients`.
 *  Falls back to the op name when the call carried nothing more. */
export function callLabel(call: InvocationCall): string {
  switch (call.family) {
    case "query":
    case "query_stream":
      return `${verbTarget(call) ?? call.op}${rowsSuffix(call)}`;
    case "fetch": {
      const where = `${call.verb ?? "GET"} ${call.target ?? "(no host)"}`;
      return call.http_status != null ? `${where} · ${call.http_status}` : where;
    }
    case "oltp":
    case "airhouse":
    case "warehouse":
    case "tx":
      return verbTarget(call) ?? call.op;
    case "email":
      return call.count != null ? plural(call.count, "recipient", "recipients") : call.op;
    case "notify":
      return call.count != null ? plural(call.count, "target", "targets") : call.op;
    case "semantic":
      return call.count != null ? plural(call.count, "measure", "measures") : call.op;
    case "airway":
    case "secrets":
      return call.target ? `${call.op} ${call.target}` : call.op;
    default:
      return call.op;
  }
}

/** How the call ended, when that is worth a word: `✕ timeout` for a failure,
 *  `held` / `refused` for what the environment's policy did instead of the
 *  call. `null` for a call that succeeded. */
export function callOutcome(call: InvocationCall): string | null {
  switch (call.status) {
    case "error":
      return `✕ ${call.error_type ?? "error"}`;
    case "held":
      return "held";
    case "refused":
      return "refused";
    default:
      return null;
  }
}

/** The bar's fill. Colour is status only — never a colour per family:
 *  a success is plain foreground, a failure is the danger tone, and a held or
 *  refused call is muted, because it is what the policy is built to do and on
 *  this surface colour means something is wrong. */
export function callBarClass(status: InvocationCall["status"]): string {
  switch (status) {
    case "error":
      return ADMIN_TONE.danger.dot;
    case "held":
    case "refused":
      return ADMIN_TONE.muted.dot;
    default:
      return "bg-foreground/30";
  }
}

/** The isolate start-up bar: a tinted, outlined surface, so it reads as the
 *  platform's time rather than a call. */
export const ISOLATE_BAR_CLASS = `${ADMIN_TONE.muted.bg} ring-1 ring-inset ${ADMIN_TONE.muted.ring}`;

/** The span the axis covers: the invocation's own duration, stretched to the
 *  last call's end and to the isolate start-up — two clocks a few
 *  microseconds apart, and a call can outlive the invocation. Never 0. */
export function timelineTotalMs(trace: Pick<InvocationCalls, "duration_ms" | "init_ms" | "calls">) {
  const ends = trace.calls.map((c) => c.start_ms + c.duration_ms);
  return Math.max(1, trace.duration_ms ?? 0, trace.init_ms ?? 0, ...ends);
}

/** Where a bar sits on the track, in percent: `left = start / total`,
 *  `width = duration / total`, never narrower than [`MIN_BAR_PCT`] and never
 *  past the right edge — a bar that would overflow is pulled back inside. */
export function barGeometry(
  startMs: number,
  durationMs: number,
  totalMs: number
): { leftPct: number; widthPct: number } {
  const total = Math.max(totalMs, 1);
  const widthPct = Math.min(100, Math.max((Math.max(durationMs, 0) / total) * 100, MIN_BAR_PCT));
  const leftPct = Math.min(Math.max((startMs / total) * 100, 0), 100 - widthPct);
  return { leftPct, widthPct };
}

export const formatMs = (ms: number): string =>
  ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)} s`;

/** The row's tooltip: op, label, outcome, duration and start offset. */
export function callTitle(call: InvocationCall): string {
  const outcome = callOutcome(call);
  return [
    call.op,
    callLabel(call),
    outcome,
    `${formatMs(call.duration_ms)}, started at +${formatMs(call.start_ms)}`
  ]
    .filter(Boolean)
    .join(" — ");
}

/** The muted line under a trace that does not show every call: the cap kept
 *  the first 200 calls to start, and calls still in flight when the
 *  invocation ended have no bar. `null` when the trace is complete. */
export function incompleteNote(
  trace: Pick<InvocationCalls, "calls" | "total_calls" | "dropped" | "unfinished">
): string | null {
  const parts: string[] = [];
  if (trace.dropped > 0) {
    parts.push(
      `Showing the first ${trace.calls.length + trace.unfinished} calls to start of ${trace.total_calls.toLocaleString()}.`
    );
  }
  if (trace.unfinished > 0) {
    parts.push(
      `${plural(trace.unfinished, "call", "calls")} had not returned when the invocation ended.`
    );
  }
  return parts.length > 0 ? parts.join(" ") : null;
}

/** Why there is no trace, in the operator's words. */
export function unrecordedText(
  reason: InvocationCallsReason | null,
  retentionDays: number
): string {
  switch (reason) {
    case "running":
      return "Still running — the call trace is written when the invocation ends.";
    case "did_not_start":
      return "This invocation never reached the function's code, so it made no ctx calls.";
    case "expired":
      return `The call trace has expired: traces are kept for ${plural(retentionDays, "day", "days")}.`;
    default:
      return "No call trace was recorded for this invocation — it ran before traces were kept, or the trace was not written.";
  }
}
