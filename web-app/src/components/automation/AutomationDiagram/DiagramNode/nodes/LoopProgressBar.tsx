/**
 * In-node progress bar for a `loop_sequential` step.
 *
 * Renders a single horizontal segmented bar (one segment per known
 * iteration status) plus a compact count summary to the right. The
 * bar is always visible during execution — no click required to see
 * "this loop is 8/30 done with 2 failures."
 *
 * Reads `LiveIteration[]` published by `useAutomationRunStream`'s
 * reducer (driven by the `subrun_step_iteration_started` /
 * `subrun_step_iteration_completed` events the automation decider
 * emits). When `iterations` is empty or undefined — the loop hasn't
 * fanned out yet, or the step isn't a loop — the bar renders nothing.
 *
 * Color tokens match `IterationGrid` for visual continuity between
 * the live in-node bar and the Retry popover's grid:
 *   - done       → emerald
 *   - failed     → destructive
 *   - cancelled  → muted-foreground
 *   - running    → primary (pulsing)
 *   - pending    → bare track (no fill)
 *
 * Click is wired by the caller (the diagram node) — we expose
 * `onClick` so a future "open live iteration grid in the sidebar"
 * affordance has a hook. The bar itself is always click-through
 * even if no handler is supplied.
 */

import type { LiveIteration } from "@/hooks/api/agentic-automations/useAgenticAutomations";

type Props = {
  iterations: LiveIteration[];
  /**
   * Total iteration count if known from the automation config. When
   * provided, "pending" iterations (not yet started) show as bare
   * track on the right; when omitted, the bar is sized to whatever
   * has been observed so far.
   */
  total?: number;
  onClick?: () => void;
};

type CountBuckets = {
  done: number;
  failed: number;
  cancelled: number;
  running: number;
  pending: number;
};

export const LoopProgressBar = ({ iterations, total, onClick }: Props) => {
  if (iterations.length === 0 && (total === undefined || total === 0)) {
    return null;
  }

  const observed = iterations.length;
  const totalKnown = total ?? observed;
  const counts = countBuckets(iterations, totalKnown);

  return (
    <button
      type='button'
      onClick={onClick}
      disabled={!onClick}
      className={`flex w-full flex-col gap-1 rounded px-1 py-0.5 text-left ${
        onClick ? "hover:bg-muted/50" : "cursor-default"
      }`}
      title={titleSummary(counts, totalKnown)}
    >
      <SegmentBar counts={counts} total={totalKnown} />
      <CountLine counts={counts} total={totalKnown} />
    </button>
  );
};

/** Single horizontal track filled by status segments. */
const SegmentBar = ({ counts, total }: { counts: CountBuckets; total: number }) => {
  // Each filled segment is a flex item with `flex: count` weighting.
  // Pending segment uses the bare track color so the bar always
  // visually conveys "out of N" — the bar width is fixed, segments
  // share it proportionally. Zero-count segments collapse to width 0
  // naturally via flex-basis.
  const segments: { count: number; tone: string }[] = [
    { count: counts.done, tone: "bg-emerald-500/70" },
    { count: counts.failed, tone: "bg-destructive/80" },
    { count: counts.cancelled, tone: "bg-muted-foreground/40" },
    { count: counts.running, tone: "animate-pulse bg-primary/70" },
    { count: counts.pending, tone: "bg-muted/60" }
  ];

  return (
    <div className='flex h-1.5 w-full overflow-hidden rounded-full bg-muted/40'>
      {segments.map((seg, i) =>
        seg.count > 0 ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: stable ordering
          <span key={i} className={seg.tone} style={{ flex: seg.count }} aria-hidden />
        ) : null
      )}
      {total === 0 && <span className='bg-muted/40' style={{ flex: 1 }} aria-hidden />}
    </div>
  );
};

const CountLine = ({ counts, total }: { counts: CountBuckets; total: number }) => {
  // Build a compact summary line. Show `done/total` always; only
  // surface the failed/cancelled/running counts when nonzero so the
  // line stays short for the common happy path.
  const parts: string[] = [];
  if (counts.failed > 0) parts.push(`${counts.failed}✗`);
  if (counts.cancelled > 0) parts.push(`${counts.cancelled}⊘`);
  if (counts.running > 0) parts.push(`${counts.running}⟳`);

  return (
    <div className='flex items-center justify-between text-[10px] text-muted-foreground tabular-nums'>
      <span>
        {counts.done}/{total}
      </span>
      {parts.length > 0 && <span>{parts.join(" · ")}</span>}
    </div>
  );
};

function countBuckets(iterations: LiveIteration[], total: number): CountBuckets {
  const counts: CountBuckets = {
    done: 0,
    failed: 0,
    cancelled: 0,
    running: 0,
    pending: 0
  };
  for (const it of iterations) {
    counts[it.status] += 1;
  }
  // Anything in the configured total that hasn't been observed yet
  // is implicitly pending.
  const accounted = counts.done + counts.failed + counts.cancelled + counts.running;
  counts.pending = Math.max(0, total - accounted);
  return counts;
}

function titleSummary(counts: CountBuckets, total: number): string {
  const parts: string[] = [`${counts.done} of ${total} done`];
  if (counts.running > 0) parts.push(`${counts.running} running`);
  if (counts.failed > 0) parts.push(`${counts.failed} failed`);
  if (counts.cancelled > 0) parts.push(`${counts.cancelled} cancelled`);
  if (counts.pending > 0) parts.push(`${counts.pending} pending`);
  return parts.join(" · ");
}
