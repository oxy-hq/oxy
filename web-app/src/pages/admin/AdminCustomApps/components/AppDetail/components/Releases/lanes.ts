import type { AdminTone } from "@/pages/admin/components/adminTone";
import type {
  ReleaseLaneName,
  ReleaseStartedBy,
  VerdictKind,
  VerdictReason
} from "@/types/appReleases";

/** Where `at` falls on the window's axis, in percent, clamped to [0, 100]. */
export function position(at: string, since: string, until: string): number {
  const t = new Date(at).getTime();
  const lo = new Date(since).getTime();
  const hi = new Date(until).getTime();
  if (![t, lo, hi].every(Number.isFinite) || hi <= lo) return 0;
  return Math.min(100, Math.max(0, ((t - lo) / (hi - lo)) * 100));
}

/** The lane an environment's events are drawn on. */
export function laneOf(environment: string): ReleaseLaneName {
  if (environment === "production") return "production";
  if (environment === "staging") return "staging";
  return "sandboxes";
}

export const LANE_LABEL: Record<ReleaseLaneName, string> = {
  production: "Production",
  staging: "Staging",
  sandboxes: "Sandboxes"
};

/**
 * Colour means something is wrong on this surface: red for worse, amber for slower,
 * and no colour at all for a release that is merely not worse — "not worse" is not
 * "healthy", and "not comparable" is not either.
 */
export function verdictTone(kind: VerdictKind): AdminTone {
  if (kind === "worse") return "danger";
  if (kind === "slower") return "warn";
  return "muted";
}

export const VERDICT_LABEL: Record<VerdictKind, string> = {
  worse: "Worse than before",
  slower: "Slower than before",
  not_worse: "Not worse",
  not_comparable: "Not comparable"
};

export const STARTED_BY_LABEL: Record<ReleaseStartedBy, string> = {
  promote: "Promoted",
  publish: "Published",
  rollback: "Rolled back",
  made_live: "Made live",
  history_start: "Serving since before history was kept",
  unpublish: "Unpublished",
  other: "Moved"
};

/** Basis points as a percentage with two decimals, from integers: 1696 → "16.96%". */
export function rateText(bp: number | null): string {
  if (bp === null) return "—";
  const sign = bp < 0 ? "-" : "";
  const abs = Math.abs(bp);
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}%`;
}

export const msText = (ms: number | null) => (ms === null ? "—" : `${ms.toLocaleString("en")} ms`);

/** CLS × 1000 as the score: 70 → "0.070". */
export function clsText(milli: number | null): string {
  if (milli === null) return "—";
  return `${Math.floor(milli / 1000)}.${String(milli % 1000).padStart(3, "0")}`;
}

/** The change from `before` to `after` in the metric's own unit, signed. */
export function deltaText(
  before: number | null,
  after: number | null,
  unit: VerdictReason["unit"]
): string | null {
  if (before === null || after === null) return null;
  const d = after - before;
  const sign = d > 0 ? "+" : d < 0 ? "−" : "±";
  const abs = Math.abs(d);
  if (unit === "bp") return `${sign}${rateText(abs).replace("%", "")} pt`;
  if (unit === "cls_milli") return `${sign}${clsText(abs)}`;
  return `${sign}${abs.toLocaleString("en")} ms`;
}

const METRIC_LABEL: Record<VerdictReason["metric"], string> = {
  failure_rate: "Failure rate",
  p95: "Route p95",
  lcp: "LCP p75",
  cls: "CLS p75"
};

const valueText = (v: number, unit: VerdictReason["unit"]) =>
  unit === "bp" ? rateText(v) : unit === "cls_milli" ? clsText(v) : msText(v);

/** "Failure rate 0.00% → 16.96%" (with the screen for a vitals rule). */
export function reasonText(r: VerdictReason): string {
  const on = r.screen ? ` on ${r.screen}` : "";
  return `${METRIC_LABEL[r.metric]}${on} ${valueText(r.before, r.unit)} → ${valueText(r.after, r.unit)}`;
}

/**
 * A build's label on the timeline. A git sha is shortened as everywhere else; any
 * other publish id is kept whole, because cutting `pulse-r4-2e7ae899f6ee` to seven
 * characters makes every release of an app read `pulse-r`.
 */
export function releaseLabel(buildId: string): string {
  return /^[0-9a-f]{11,}$/i.test(buildId) ? buildId.slice(0, 7) : buildId;
}
