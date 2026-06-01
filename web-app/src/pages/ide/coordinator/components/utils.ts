/** Pure helpers shared across the coordinator dashboard. */

// ── Duration & time formatting ──────────────────────────────────────────────

/** Compact duration between two ISO timestamps (e.g. "2m 11s"). */
export const formatDuration = (startIso: string, endIso: string): string => {
  const ms = new Date(endIso).getTime() - new Date(startIso).getTime();
  return formatDurationMs(ms);
};

export const formatDurationMs = (ms: number): string => {
  const secs = Math.max(0, Math.floor(ms / 1000));
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${secs % 60}s`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ${mins % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
};

/** Relative-from-now label (e.g. "3m ago", "in 12m"). */
export const formatRelative = (iso: string | null | undefined): string => {
  if (!iso) return "—";
  const diff = new Date(iso).getTime() - Date.now();
  const future = diff > 0;
  const secs = Math.abs(Math.floor(diff / 1000));
  let body: string;
  if (secs < 60) body = `${secs}s`;
  else if (secs < 3600) body = `${Math.floor(secs / 60)}m`;
  else if (secs < 86400) body = `${Math.floor(secs / 3600)}h`;
  else body = `${Math.floor(secs / 86400)}d`;
  return future ? `in ${body}` : `${body} ago`;
};

/** Short absolute timestamp (e.g. "May 23, 14:00"). */
export const formatTimestamp = (iso: string | null | undefined): string => {
  if (!iso) return "—";
  const d = new Date(iso);
  return `${d.toLocaleDateString(undefined, { month: "short", day: "numeric" })}, ${d.toLocaleTimeString(
    undefined,
    { hour: "2-digit", minute: "2-digit" }
  )}`;
};

/** Truncate the leading segment of a run/task id for dense display. */
export const shortId = (id: string): string => id.slice(0, 8);

/** Short USD: "$0.0023" / "$1.42" / "$12.40". 4 decimals under $0.01 so a
 *  tiny LLM call doesn't render as "$0.00". */
export const formatUsd = (n: number): string => {
  if (n < 0.01) return `$${n.toFixed(4)}`;
  return `$${n.toFixed(2)}`;
};

/** Compact token count: "847" / "12.4k" / "1.2M". Mirrors the bucket
 *  thresholds in `LlmUsageCard.formatTokens` so the runs list and run
 *  detail format the same number the same way. */
export const formatTokens = (n: number): string => {
  if (n < 1000) return n.toLocaleString();
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(n < 10_000_000 ? 2 : 1)}M`;
};

// ── Cron evaluation ─────────────────────────────────────────────────────────

/** Expand one cron field (wildcard, step, range, or list) into a set of ints. */
const parseCronField = (field: string, min: number, max: number): Set<number> => {
  const out = new Set<number>();
  for (const part of field.split(",")) {
    let step = 1;
    let range = part;
    const slash = part.indexOf("/");
    if (slash !== -1) {
      step = Number.parseInt(part.slice(slash + 1), 10) || 1;
      range = part.slice(0, slash);
    }
    let lo = min;
    let hi = max;
    if (range !== "*") {
      const dash = range.indexOf("-");
      if (dash !== -1) {
        lo = Number.parseInt(range.slice(0, dash), 10);
        hi = Number.parseInt(range.slice(dash + 1), 10);
      } else {
        lo = hi = Number.parseInt(range, 10);
      }
    }
    if (Number.isNaN(lo) || Number.isNaN(hi)) continue;
    for (let n = lo; n <= hi; n += step) out.add(n);
  }
  return out;
};

/** Wall-clock offset (minutes) of an IANA timezone at a given instant. */
const tzOffsetMinutes = (date: Date, tz: string): number => {
  try {
    const dtf = new Intl.DateTimeFormat("en-US", {
      timeZone: tz,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false
    });
    const map: Record<string, number> = {};
    for (const p of dtf.formatToParts(date)) {
      if (p.type !== "literal") map[p.type] = Number.parseInt(p.value, 10);
    }
    const asUtc = Date.UTC(
      map.year,
      map.month - 1,
      map.day,
      map.hour === 24 ? 0 : map.hour,
      map.minute,
      map.second
    );
    return (asUtc - date.getTime()) / 60000;
  } catch {
    return 0;
  }
};

/**
 * Compute the next `count` fire times of a standard 5-field cron expression
 * interpreted in `tz`. The timezone offset is sampled once, so DST shifts
 * inside the preview window are approximate — fine for a next-runs preview.
 */
export const cronNextRuns = (
  expr: string,
  tz: string,
  count: number,
  from: Date = new Date()
): Date[] => {
  const fields = expr.trim().split(/\s+/);
  if (fields.length < 5) return [];
  const minutes = parseCronField(fields[0], 0, 59);
  const hours = parseCronField(fields[1], 0, 23);
  const doms = parseCronField(fields[2], 1, 31);
  const months = parseCronField(fields[3], 1, 12);
  const daysOfWeek = parseCronField(fields[4], 0, 7);
  const domRestricted = fields[2] !== "*";
  const dowRestricted = fields[4] !== "*";

  const offset = tzOffsetMinutes(from, tz);
  let t = Math.ceil((from.getTime() + offset * 60000) / 60000) * 60000 + 60000;
  const out: Date[] = [];
  const limit = 367 * 24 * 60;
  for (let i = 0; i < limit && out.length < count; i++) {
    const d = new Date(t);
    const dow = d.getUTCDay();
    const dowMatch = daysOfWeek.has(dow) || (dow === 0 && daysOfWeek.has(7));
    const dom = d.getUTCDate();
    let dayOk: boolean;
    if (domRestricted && dowRestricted) dayOk = doms.has(dom) || dowMatch;
    else dayOk = (!domRestricted || doms.has(dom)) && (!dowRestricted || dowMatch);
    if (
      months.has(d.getUTCMonth() + 1) &&
      dayOk &&
      hours.has(d.getUTCHours()) &&
      minutes.has(d.getUTCMinutes())
    ) {
      out.push(new Date(t - offset * 60000));
    }
    t += 60000;
  }
  return out;
};

/** Count cron occurrences in [start, end) — used for backfill blast radius. */
export const cronCountBetween = (
  expr: string,
  tz: string,
  start: Date,
  end: Date,
  cap = 1000
): number => {
  if (end <= start) return 0;
  const runs = cronNextRuns(expr, tz, cap, start);
  return runs.filter((r) => r.getTime() < end.getTime()).length;
};

/** Best-effort human summary of a 5-field cron expression. */
export const describeCron = (expr: string): string => {
  const f = expr.trim().split(/\s+/);
  if (f.length < 5) return expr;
  const [min, hour, dom, , dow] = f;
  const hhmm = (h: string, m: string) => `${h.padStart(2, "0")}:${m.padStart(2, "0")}`;
  const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  if (min.startsWith("*/")) return `Every ${min.slice(2)} minutes`;
  if (hour === "*" && dom === "*" && dow === "*" && /^\d+$/.test(min))
    return `Hourly at :${min.padStart(2, "0")}`;
  if (/^\d+$/.test(min) && /^\d+$/.test(hour)) {
    if (dom === "*" && dow === "*") return `Daily at ${hhmm(hour, min)}`;
    if (dom === "*" && /^\d+$/.test(dow))
      return `Weekly on ${days[Number(dow) % 7]} at ${hhmm(hour, min)}`;
    if (/^\d+$/.test(dom) && dow === "*") return `Monthly on day ${dom} at ${hhmm(hour, min)}`;
  }
  return expr;
};
