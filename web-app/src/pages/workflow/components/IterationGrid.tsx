/**
 * Iteration override grid for the Retry popover.
 *
 * Pulls each loop step's `iterations` map from the prior run's
 * snapshot and renders one cell per iteration as a dense colored
 * square — no inline label. A summary line above the strip gives the
 * per-status counts at a glance; hovering a cell exposes the index,
 * value, status, and error (when present) via the native title
 * tooltip; clicking a cached cell flips it to "force re-run".
 *
 * For larger loops (configurable threshold, currently 30) a search +
 * status-filter row appears above the grids so users can find a
 * specific value or narrow to just failures without scanning hundreds
 * of cells. Search matches numeric indices first (`"27"` jumps to
 * iteration #27) and falls through to a case-insensitive substring
 * match on the iteration value.
 *
 * For pre-A runs (no `iterations` map in the snapshot), the grid is
 * empty — the user just gets the existing "Reuse unchanged steps"
 * toggle.
 */

import { Search } from "lucide-react";
import { useMemo, useState } from "react";

import { Input } from "@/components/ui/shadcn/input";
import type { LiveIteration } from "@/hooks/api/agentic-workflows/useAgenticWorkflows";
import type { IterationOutcome } from "@/services/api/agenticWorkflows";

type Props = {
  /** Keyed by step name. Each step has its `iterations` map flattened. */
  steps: Record<string, IterationOutcome[]>;
  /**
   * `"edit"` (default, Retry popover): force-replay cells are
   * clickable; `forced`/`onChange` are required.
   * `"view"` (sidebar live): cells are read-only; `forced` is
   * always empty, `onChange` is ignored. Running cells render with
   * a primary pulse to indicate in-flight work.
   */
  mode?: "edit" | "view";
  /** Per-step indices the user has toggled to force-replay. */
  forced?: Record<string, number[]>;
  onChange?: (next: Record<string, number[]>) => void;
};

type CellState = "done" | "failed" | "cancelled" | "running" | "forced";

type StatusFilter = "all" | CellState;

/** Threshold above which the search + status-filter row appears. */
const FILTER_THRESHOLD = 30;

export const IterationGrid = ({ steps, mode = "edit", forced, onChange }: Props) => {
  // In view mode the parent passes neither `forced` nor `onChange`;
  // collapse to safe defaults so the cell render path stays unified.
  const forcedMap = forced ?? {};
  const isView = mode === "view";

  const stepNames = Object.keys(steps);
  const totalIterations = useMemo(
    () => stepNames.reduce((n, s) => n + steps[s].length, 0),
    [stepNames, steps]
  );
  const showFilters = totalIterations >= FILTER_THRESHOLD;
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<StatusFilter>("all");

  if (stepNames.length === 0) return null;

  const toggle = (step: string, index: number) => {
    if (isView || !onChange) return;
    const currentSet = new Set(forcedMap[step] ?? []);
    if (currentSet.has(index)) {
      currentSet.delete(index);
    } else {
      currentSet.add(index);
    }
    const next = { ...forcedMap };
    if (currentSet.size === 0) {
      delete next[step];
    } else {
      next[step] = [...currentSet].sort((a, b) => a - b);
    }
    onChange(next);
  };

  // Filter chip set + footer hint differ between modes — view mode
  // never has "forced" cells (no toggle) and the hint is purely
  // informational rather than instructional.
  const filterChips: StatusFilter[] = isView
    ? ["all", "running", "failed", "cancelled", "done"]
    : ["all", "failed", "cancelled", "done", "forced"];
  const title = isView ? "Iterations" : "Iteration overrides";

  return (
    <div className='space-y-3'>
      <div className='flex items-baseline justify-between gap-2'>
        <p className='font-medium text-sm'>{title}</p>
        {showFilters && (
          <p className='text-[11px] text-muted-foreground'>{totalIterations} total</p>
        )}
      </div>

      {showFilters && (
        <div className='space-y-2'>
          <div className='relative'>
            <Search className='absolute top-1/2 left-2 size-3 -translate-y-1/2 text-muted-foreground' />
            <Input
              placeholder='Find by value or index'
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className='h-7 pl-7 text-xs'
            />
          </div>
          <div className='flex flex-wrap gap-1'>
            {filterChips.map((chip) => (
              <FilterChip key={chip} active={filter === chip} onClick={() => setFilter(chip)}>
                {chip}
              </FilterChip>
            ))}
          </div>
        </div>
      )}

      {stepNames.map((step) => {
        const sorted = [...steps[step]].sort((a, b) => a.index - b.index);
        const forcedSet = new Set(forcedMap[step] ?? []);
        const counts = countByCellState(sorted, forcedSet);
        const total = sorted.length;
        const visible = sorted.filter((iter) => matchesFilters(iter, forcedSet, query, filter));
        return (
          <div key={step} className='space-y-1'>
            <div className='flex items-baseline justify-between gap-2'>
              <p className='truncate font-mono text-foreground text-xs'>{step}</p>
              <p className='shrink-0 text-[11px] text-muted-foreground'>
                {showFilters && visible.length !== total ? (
                  <>
                    {visible.length}/{total} ·{" "}
                  </>
                ) : null}
                <CountSummary counts={counts} total={total} />
              </p>
            </div>
            {visible.length === 0 ? (
              <p className='text-[11px] text-muted-foreground italic'>No matches</p>
            ) : (
              <div className='flex max-h-32 flex-wrap gap-0.5 overflow-y-auto'>
                {visible.map((iter) => {
                  const state: CellState = forcedSet.has(iter.index)
                    ? "forced"
                    : (iter.status as CellState);
                  return (
                    <IterationCell
                      key={iter.index}
                      iter={iter}
                      state={state}
                      readOnly={isView}
                      onClick={() => toggle(step, iter.index)}
                    />
                  );
                })}
              </div>
            )}
          </div>
        );
      })}

      <p className='text-[11px] text-muted-foreground leading-relaxed'>
        {isView
          ? "Hover for the iteration's value and status. Running iterations show in primary; failed in red."
          : "Failed / cancelled iterations always re-run. Click a cached cell to force its replay. Hover for the iteration's value and status."}
      </p>
    </div>
  );
};

const FilterChip = ({
  active,
  onClick,
  children
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) => (
  <button
    type='button'
    onClick={onClick}
    className={`rounded border px-1.5 py-0.5 text-[11px] capitalize transition-colors ${
      active
        ? "border-foreground/30 bg-foreground/10 text-foreground"
        : "border-muted-foreground/20 bg-transparent text-muted-foreground hover:bg-muted"
    }`}
  >
    {children}
  </button>
);

const CountSummary = ({ counts, total }: { counts: Record<CellState, number>; total: number }) => {
  const parts: string[] = [];
  if (counts.done > 0) parts.push(`${counts.done} cached`);
  if (counts.failed > 0) parts.push(`${counts.failed} failed`);
  if (counts.cancelled > 0) parts.push(`${counts.cancelled} cancelled`);
  if (counts.running > 0) parts.push(`${counts.running} running`);
  if (counts.forced > 0) parts.push(`${counts.forced} forced`);
  if (parts.length === 0) return <>{total} iterations</>;
  return <>{parts.join(" · ")}</>;
};

const IterationCell = ({
  iter,
  state,
  readOnly,
  onClick
}: {
  iter: IterationOutcome;
  state: CellState;
  readOnly: boolean;
  onClick: () => void;
}) => {
  // Edit mode: failed/cancelled cells always re-run anyway, so they
  // disable the click but keep the tooltip. Read-only (view) mode
  // disables every cell uniformly — nothing is toggleable from there.
  const interactive = !readOnly && state !== "failed" && state !== "cancelled";
  const tone = TONE[state];
  const label = describe(iter, state, readOnly);

  return (
    <button
      type='button'
      onClick={onClick}
      disabled={!interactive}
      title={label}
      aria-label={label}
      className={`size-3.5 rounded-[3px] border transition-transform ${tone} ${
        interactive ? "cursor-pointer hover:scale-110 hover:brightness-110" : "cursor-default"
      }`}
    />
  );
};

const TONE: Record<CellState, string> = {
  done: "border-emerald-500/40 bg-emerald-500/30",
  failed: "border-destructive/50 bg-destructive/40",
  cancelled: "border-muted-foreground/40 bg-muted-foreground/20",
  running: "animate-pulse border-primary/60 bg-primary/50",
  forced: "border-amber-500/60 bg-amber-500/40 ring-1 ring-amber-500/50"
};

function describe(iter: IterationOutcome, state: CellState, readOnly: boolean): string {
  const valueLabel = formatValue(iter.value);
  const head = `#${iter.index} · ${valueLabel}`;
  // Read-only (live view) tooltips drop the actionable hints
  // ("click to …", "will re-run") since the cells can't be clicked.
  switch (state) {
    case "done":
      return readOnly ? `${head} · done` : `${head} · cached (will reuse) — click to force re-run`;
    case "failed":
      return `${head} · failed${iter.error ? `\n${iter.error}` : ""}${
        readOnly ? "" : " — will re-run"
      }`;
    case "cancelled":
      return `${head} · cancelled${readOnly ? "" : " — will re-run"}`;
    case "running":
      // Only reachable in view mode (snapshot-derived outcomes
      // never carry "running"), so the tooltip is purely
      // informational.
      return `${head} · running…`;
    case "forced":
      return `${head} · forced re-run — click to undo`;
  }
}

/** Compact display for a serialized iteration value. */
function formatValue(value: unknown): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "string") return value.length > 40 ? `${value.slice(0, 37)}…` : value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    const s = JSON.stringify(value);
    return s.length > 40 ? `${s.slice(0, 37)}…` : s;
  } catch {
    return "?";
  }
}

/**
 * Lowercase JSON serialisation of the iteration value for substring
 * matching. Numbers and booleans land as their natural string form.
 */
function searchableValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value.toLowerCase();
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  try {
    return JSON.stringify(value).toLowerCase();
  } catch {
    return "";
  }
}

function matchesFilters(
  iter: IterationOutcome,
  forcedSet: Set<number>,
  query: string,
  filter: StatusFilter
): boolean {
  // Status filter: match against the effective state (forced overrides
  // the raw `done` status).
  if (filter !== "all") {
    const effective: CellState = forcedSet.has(iter.index) ? "forced" : (iter.status as CellState);
    if (effective !== filter) return false;
  }

  const trimmed = query.trim();
  if (!trimmed) return true;

  // Numeric query: match exact index (`27` jumps to iteration #27).
  // Falls through to value substring if not an integer or doesn't match
  // — e.g., "100" with no #100 should still match values containing
  // "100".
  if (/^\d+$/.test(trimmed) && iter.index === Number(trimmed)) {
    return true;
  }

  return searchableValue(iter.value).includes(trimmed.toLowerCase());
}

function countByCellState(
  iters: IterationOutcome[],
  forcedSet: Set<number>
): Record<CellState, number> {
  const counts: Record<CellState, number> = {
    done: 0,
    failed: 0,
    cancelled: 0,
    running: 0,
    forced: 0
  };
  for (const it of iters) {
    if (forcedSet.has(it.index)) {
      counts.forced += 1;
    } else if (it.status === "running") {
      counts.running += 1;
    } else if (it.status === "done") {
      counts.done += 1;
    } else if (it.status === "failed") {
      counts.failed += 1;
    } else {
      counts.cancelled += 1;
    }
  }
  return counts;
}

/**
 * Lift the SSE-driven `LiveIteration[]` shape (used by the loop's
 * in-node progress bar) into the snapshot-shaped
 * `Record<step, IterationOutcome[]>` that `IterationGrid` consumes
 * — same component, two data sources. Step name keys the entry so
 * the grid renders one labeled section per loop.
 */
export function liveIterationsToOutcomes(
  stepName: string,
  iterations: LiveIteration[]
): Record<string, IterationOutcome[]> {
  if (iterations.length === 0) return {};
  return {
    [stepName]: iterations.map((it) => ({
      value: it.value,
      index: it.index,
      status: it.status,
      // `answer` is the snapshot-side "result of this iteration"
      // — live iterations don't carry it; the grid's tooltip
      // gracefully omits it when undefined.
      error: it.error
    }))
  };
}

/**
 * Extract per-loop-step iteration maps from a workflow run snapshot's
 * `results` object. Skips non-loop steps and pre-A runs (no `iterations`
 * sub-map). Caller passes the resulting `Record<step, IterationOutcome[]>`
 * to `IterationGrid`.
 */
export function extractIterationsBySteps(
  results: Record<string, unknown> | null | undefined
): Record<string, IterationOutcome[]> {
  if (!results) return {};
  const out: Record<string, IterationOutcome[]> = {};
  for (const [step, value] of Object.entries(results)) {
    if (!value || typeof value !== "object") continue;
    const itersRaw = (value as Record<string, unknown>).iterations;
    if (!itersRaw || typeof itersRaw !== "object") continue;
    const entries = Object.values(itersRaw as Record<string, unknown>)
      .filter((e): e is Record<string, unknown> => !!e && typeof e === "object")
      .filter(
        (e): e is IterationOutcome =>
          typeof e.index === "number" &&
          typeof e.status === "string" &&
          (e.status === "done" || e.status === "failed" || e.status === "cancelled")
      );
    if (entries.length > 0) out[step] = entries;
  }
  return out;
}
