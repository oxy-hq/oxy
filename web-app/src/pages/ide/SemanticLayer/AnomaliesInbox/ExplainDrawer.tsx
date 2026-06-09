import { ArrowUp, ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle
} from "@/components/ui/shadcn/sheet";
import { Spinner } from "@/components/ui/shadcn/spinner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { useAnomalyExplain, useRefreshAnomalyExplain } from "@/hooks/api/useMetricAnomalies";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import { ExplainNodeRow, splitLabel } from "@/pages/ide/MetricTree/components/ExplainTree";
import useCurrentOrg from "@/stores/useCurrentOrg";
import type { MetricAnomaly } from "@/types/metricAnomalies";
import type {
  DriverAttribution,
  ExplainNode,
  ExplainResult,
  ExplainWarning
} from "@/types/metricTree";
import ExplainGraph from "./ExplainGraph";

interface Props {
  anomaly: MetricAnomaly | null;
  /** Called when the user closes the drawer (X, escape, or outside click). */
  onOpenChange: (open: boolean) => void;
}

/**
 * Side drawer that calls `POST /semantic/metric-tree/explain` directly for
 * the given anomaly and renders the decomposition tree. Skips the analytics
 * agent entirely — deterministic, fast, no LLM ambiguity.
 *
 * Periods are derived from the anomaly:
 *   current  = (period_start, period_start)         — single bucket
 *   previous = (period_start - 1 season, same)      — for the period-over-period
 *
 * The same-cycle-back baseline lines up with the detector's `seasonality`
 * config, so the agent's explain compares the anomaly bucket against the
 * matching prior bucket (e.g. Sat vs Sat for daily / weekly seasonality).
 *
 * A "Ask a follow-up" button opens the home chat with the explain result
 * summarized so the agent can answer follow-up questions with full context.
 */
export default function ExplainDrawer({ anomaly, onOpenChange }: Props) {
  const navigate = useNavigate();
  const { project } = useCurrentProjectBranch();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const [followUp, setFollowUp] = useState("");

  const periods = useMemo(() => (anomaly ? deriveExplainPeriods(anomaly) : null), [anomaly]);

  // Server-cached explain. The first time we hit `/explain` for this
  // anomaly, airlayer runs the recursive search and the result is
  // written onto the row's `explain_cache` JSONB column. Every later
  // call — including after a page refresh — returns the cached payload
  // instantly. The cache lifecycle is tied to the anomaly row.
  const explain = useAnomalyExplain(anomaly?.id ?? null);
  const refresh = useRefreshAnomalyExplain();

  // Clear the follow-up textarea whenever the user switches anomalies.
  // `anomaly?.id` is the discriminator — same id = same row = keep draft.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional, only react to row id changes
  useEffect(() => {
    setFollowUp("");
  }, [anomaly?.id]);

  if (!anomaly) return null;

  const homePath = ROUTES.ORG(orgSlug).WORKSPACE(project.id).HOME;
  const canSubmit = !!explain.data && followUp.trim().length > 0;

  const submitFollowUp = () => {
    if (!canSubmit) return;
    navigate(homePath, {
      state: {
        prefillQuestion: buildFollowUpPrompt(
          anomaly,
          periods,
          explain.data ?? null,
          followUp.trim()
        ),
        autoSubmit: true
      }
    });
    onOpenChange(false);
  };

  /** Plain Enter submits (matches the rest of the app's MessageInput).
   *  Shift+Enter inserts a newline. */
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submitFollowUp();
    }
  };

  return (
    <Sheet open={!!anomaly} onOpenChange={onOpenChange}>
      {/* gap-0 + p-0 strip the default SheetContent spacing — we own the
          internal stack so the body, observed-vs-expected card, and footer
          line up edge-to-edge without the double-padding the defaults give. */}
      <SheetContent side='right' className='flex w-full flex-col gap-0 p-0 sm:max-w-xl'>
        <SheetHeader className='relative border-border border-b pb-3'>
          {/* Refresh: bare icon button to match SheetContent's own X
              close — which sits at `top-4 right-4` and is just a 16px
              icon with opacity-70. Placing ours at `top-4 right-12`
              with the same shape keeps both controls visually aligned. */}
          {explain.data && (
            <button
              type='button'
              onClick={() => refresh.mutate(anomaly.id)}
              disabled={refresh.isPending}
              aria-label='Refresh explain'
              className='absolute top-4 right-12 rounded-xs opacity-70 transition-opacity hover:opacity-100 disabled:pointer-events-none disabled:opacity-50'
            >
              <RefreshCw className={refresh.isPending ? "size-4 animate-spin" : "size-4"} />
            </button>
          )}
          <SheetTitle className='pr-20'>{anomaly.label || anomaly.measure}</SheetTitle>
          <SheetDescription className='t-code pr-20 text-xs'>
            {anomaly.measure} · {anomaly.period_start.slice(0, 10)} · {anomaly.granularity}
          </SheetDescription>
        </SheetHeader>

        <div className='flex flex-1 flex-col gap-3 overflow-hidden p-4'>
          <ObservedVsExpected anomaly={anomaly} />
          {explain.isPending && (
            <div className='flex h-32 items-center justify-center'>
              <Spinner />
            </div>
          )}
          {explain.error && (
            <p className='text-destructive text-sm'>
              {explain.error instanceof Error
                ? explain.error.message
                : "Failed to run the explain decomposition."}
            </p>
          )}
          {explain.data && <ExplainBody result={explain.data} />}
        </div>

        {/* Compose a follow-up. Submit ships <explain context> + <user
            question> to the home chat so the agent answers with full
            anomaly + decomposition context, not from scratch. */}
        <SheetFooter className='gap-1.5 border-border border-t'>
          <div className='relative'>
            <Textarea
              value={followUp}
              onChange={(e) => setFollowUp(e.target.value)}
              onKeyDown={onKeyDown}
              placeholder='Ask a follow-up — the explain result is sent as context'
              disabled={!explain.data}
              className='min-h-[60px] resize-none border-border bg-secondary pr-12 text-sm'
            />
            <Button
              size='icon'
              variant='default'
              onClick={submitFollowUp}
              disabled={!canSubmit}
              aria-label='Send follow-up'
              className='absolute top-1/2 right-2 -translate-y-1/2 transform'
            >
              <ArrowUp />
            </Button>
          </div>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  );
}

function ObservedVsExpected({ anomaly }: { anomaly: MetricAnomaly }) {
  const deltaPct =
    Math.abs(anomaly.expected) < 1e-9
      ? null
      : ((anomaly.observed - anomaly.expected) / Math.abs(anomaly.expected)) * 100;
  return (
    <div className='grid grid-cols-3 gap-2 rounded-md border border-border bg-muted/40 p-3'>
      <Stat label='Observed' value={formatNumber(anomaly.observed)} />
      <Stat label='Expected' value={formatNumber(anomaly.expected)} />
      <Stat
        label='Δ'
        value={
          deltaPct === null
            ? "—"
            : `${anomaly.observed >= anomaly.expected ? "+" : ""}${deltaPct.toFixed(1)}%`
        }
      />
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className='flex flex-col'>
      <span className='text-muted-foreground text-xs'>{label}</span>
      <span className='t-code font-medium text-sm'>{value}</span>
    </div>
  );
}

export function ExplainBody({ result }: { result: ExplainResult }) {
  const [warningsOpen, setWarningsOpen] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const [bodyHeight, setBodyHeight] = useState(0);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => setBodyHeight(entry.contentRect.height));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Height of elements above the graph inside ExplainBody:
  // CoverageLine (~24px) + gap (12px) + Section label (~16px) + gap (6px) + TabsList (32px) + gap (8px) = ~98px
  const GRAPH_OVERHEAD = 100;
  const graphHeight = Math.max(180, bodyHeight - GRAPH_OVERHEAD);

  return (
    <div ref={bodyRef} className='flex min-h-0 flex-1 flex-col gap-3 overflow-auto'>
      <CoverageLine result={result} />
      {result.warnings && result.warnings.length > 0 && (
        <div>
          <button
            type='button'
            onClick={() => setWarningsOpen((o) => !o)}
            className='flex items-center gap-1 text-orange-600 text-xs hover:underline dark:text-orange-400'
          >
            {warningsOpen ? (
              <ChevronDown className='size-3' />
            ) : (
              <ChevronRight className='size-3' />
            )}
            {result.warnings.length} warning{result.warnings.length !== 1 ? "s" : ""}
          </button>
          {warningsOpen && (
            <ul className='mt-1 flex flex-col gap-1'>
              {result.warnings.map((w, i) => (
                <li
                  key={i}
                  className='rounded-md border border-orange-500/40 bg-orange-500/10 p-2 text-orange-700 text-xs dark:text-orange-300'
                >
                  {warningMessage(w)}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {result.nodes.length === 0 ? (
        <p className='text-muted-foreground text-sm'>
          No decomposition found — the move didn't concentrate in any single component or dimension
          above the significance threshold.
        </p>
      ) : (
        <Section title='Decomposition' className='min-h-0 flex-1'>
          {/* Graph view first — clicks/highlights the same data; the List
              tab is the fallback for screen readers and copy/paste. */}
          <Tabs defaultValue='graph' className='flex min-h-0 flex-1 flex-col gap-2'>
            <TabsList className='w-fit'>
              <TabsTrigger value='graph'>Graph</TabsTrigger>
              <TabsTrigger value='list'>List</TabsTrigger>
            </TabsList>
            <TabsContent
              value='graph'
              className='relative overflow-hidden rounded-md border border-border bg-muted/20'
            >
              <ExplainGraph result={result} height={graphHeight} />
            </TabsContent>
            <TabsContent value='list'>
              <ul className='flex flex-col gap-1'>
                {result.nodes.map((n, i) => (
                  <ExplainNodeRow
                    key={`${n.measure}-${i}`}
                    node={n}
                    depth={0}
                    parentDelta={result.target_delta}
                  />
                ))}
              </ul>
            </TabsContent>
          </Tabs>
        </Section>
      )}
      {result.driver_attribution && result.driver_attribution.length > 0 && (
        <Section title='Driver attribution'>
          <ul className='flex flex-col gap-1'>
            {result.driver_attribution.map((d, i) => (
              <DriverRow key={i} driver={d} />
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}

function Section({
  title,
  children,
  className
}: {
  title: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={`flex flex-col gap-1.5${className ? ` ${className}` : ""}`}>
      <p className='text-muted-foreground text-xs uppercase tracking-wide'>{title}</p>
      {children}
    </div>
  );
}

/** Period-over-period delta + coverage % beneath the observed/expected card. */
function CoverageLine({ result }: { result: ExplainResult }) {
  const deltaSign = result.target_delta >= 0 ? "+" : "";
  return (
    <p className='text-muted-foreground text-xs'>
      Period-over-period:{" "}
      <span className='t-code text-foreground'>
        {formatNumber(result.target_previous)} → {formatNumber(result.target_current)}
      </span>{" "}
      ({deltaSign}
      {formatNumber(result.target_delta)})
    </p>
  );
}

function DriverRow({ driver }: { driver: DriverAttribution }) {
  const impact = driver.estimated_target_impact;
  return (
    <li className='rounded-md border border-border bg-card p-2 text-sm'>
      <div className='flex items-center justify-between gap-2'>
        <span className='font-medium'>{driver.driver_measure}</span>
        {impact !== undefined && impact !== null && (
          <span className='text-muted-foreground text-xs tabular-nums'>
            est. impact {impact >= 0 ? "+" : ""}
            {formatNumber(impact)}
          </span>
        )}
      </div>
      <p className='text-muted-foreground text-xs'>
        Δ {driver.driver_delta >= 0 ? "+" : ""}
        {formatNumber(driver.driver_delta)} ({formatNumber(driver.driver_previous)} →{" "}
        {formatNumber(driver.driver_current)})
        {driver.coefficient !== undefined &&
          driver.coefficient !== null &&
          ` · coef ${driver.coefficient}`}
        {driver.form && ` · ${driver.form}`}
      </p>
      {driver.description && (
        <p className='mt-1 text-muted-foreground text-xs italic'>{driver.description}</p>
      )}
    </li>
  );
}

// ── derivations ────────────────────────────────────────────────────────────

interface DerivedPeriods {
  current: [string, string];
  previous: [string, string];
}

/** Build (current, previous) period tuples for the explain call.
 *  current = the anomaly bucket; previous = one season back. */
function deriveExplainPeriods(a: MetricAnomaly): DerivedPeriods {
  const current = a.period_start.slice(0, 10);
  const baseline = sameCyclePrior(a);
  return { current: [current, current], previous: [baseline, baseline] };
}

function sameCyclePrior(a: MetricAnomaly): string {
  const start = new Date(a.period_start);
  const out = new Date(start);
  switch (a.granularity) {
    case "day":
    case "week":
      out.setUTCDate(out.getUTCDate() - 7);
      break;
    case "month":
      out.setUTCMonth(out.getUTCMonth() - 1);
      break;
    default:
      out.setUTCDate(out.getUTCDate() - 7);
  }
  return out.toISOString().slice(0, 10);
}

/** Build the chat prompt: a fenced "context" block with the full
 *  decomposition (anomaly summary, period-over-period, recursive split
 *  tree, driver attributions, warnings) followed by the user's literal
 *  question. The agent sees the context as background; the question
 *  itself is what it answers.
 *
 *  We dump quite a bit so the agent can answer "which child segment
 *  drove the parent component?" without re-running the decomposition. */
function buildFollowUpPrompt(
  anomaly: MetricAnomaly,
  periods: DerivedPeriods | null,
  result: ExplainResult | null,
  userQuestion: string
): string {
  const ctx: string[] = [
    `Anomaly: ${anomaly.label || anomaly.measure} (${anomaly.measure})`,
    `Bucket: ${anomaly.period_start.slice(0, 10)} (${anomaly.granularity})`,
    `Observed ${formatNumber(anomaly.observed)} vs expected baseline ${formatNumber(anomaly.expected)} (${anomaly.severity} severity, z=${anomaly.z_score.toFixed(2)})`
  ];
  if (periods) {
    ctx.push(`Period-over-period: current=${periods.current[0]}, previous=${periods.previous[0]}`);
  }
  if (result) {
    const deltaSign = result.target_delta >= 0 ? "+" : "";
    ctx.push(
      `Target moved ${formatNumber(result.target_previous)} → ${formatNumber(result.target_current)} (${deltaSign}${formatNumber(result.target_delta)}); ${(result.coverage * 100).toFixed(0)}% of the delta is explained by the decomposition below.`
    );
    if (result.nodes.length > 0) {
      ctx.push("");
      ctx.push("Decomposition tree (each line = one split; indent = nesting):");
      for (const n of result.nodes) {
        appendNodeLines(ctx, n, 0);
      }
    }
    if (result.driver_attribution && result.driver_attribution.length > 0) {
      ctx.push("");
      ctx.push("Declared drivers (causal/correlative inputs from the metric tree):");
      for (const d of result.driver_attribution) {
        const driverSign = d.driver_delta >= 0 ? "+" : "";
        const impact = d.estimated_target_impact;
        const impactStr =
          impact !== undefined && impact !== null
            ? ` → est. target impact ${impact >= 0 ? "+" : ""}${formatNumber(impact)}`
            : "";
        ctx.push(
          `  • ${d.driver_measure}: Δ ${driverSign}${formatNumber(d.driver_delta)} (${formatNumber(d.driver_previous)} → ${formatNumber(d.driver_current)})${d.coefficient !== undefined && d.coefficient !== null ? ` · coef ${d.coefficient}` : ""} · ${d.form}${impactStr}`
        );
        if (d.description) {
          ctx.push(`    note: ${d.description}`);
        }
      }
    }
    if (result.warnings && result.warnings.length > 0) {
      ctx.push("");
      ctx.push("Detector warnings on this decomposition:");
      for (const w of result.warnings) {
        ctx.push(`  • ${warningMessage(w)}`);
      }
    }
  }
  return ["```context", ...ctx, "```", "", userQuestion].join("\n");
}

/** Recursively append one indented line per node + its siblings + its
 *  recursive children to the context buffer. Mirrors the visual
 *  Decomposition tree the user sees in the drawer so the agent works
 *  off the same structure. */
function appendNodeLines(ctx: string[], node: ExplainNode, depth: number): void {
  const indent = "  ".repeat(depth + 1);
  const deltaSign = node.delta >= 0 ? "+" : "";
  ctx.push(
    `${indent}• ${splitLabel(node.split)} (measure ${node.measure}) — Δ ${deltaSign}${formatNumber(node.delta)} · ${(node.root_fraction * 100).toFixed(1)}% of root · concentration ${(node.concentration * 100).toFixed(0)}%`
  );
  // Surface siblings inline so the agent knows the next-best alternatives
  // at the same level without us having to recurse into them.
  if (node.siblings && node.siblings.length > 0) {
    const sibSummary = node.siblings
      .slice(0, 4)
      .map((s) => `${splitLabel(s.split)} (${(s.root_fraction * 100).toFixed(1)}%)`)
      .join("; ");
    ctx.push(`${indent}  also considered: ${sibSummary}`);
  }
  if (node.children) {
    for (const child of node.children) {
      appendNodeLines(ctx, child, depth + 1);
    }
  }
}

/** Render an [`ExplainWarning`] as a single human-readable sentence. */
function warningMessage(w: ExplainWarning): string {
  switch (w.type) {
    case "simpsons_paradox":
      return `Simpson's paradox on ${w.dimension}: aggregate moved ${
        w.aggregate_delta >= 0 ? "+" : ""
      }${formatNumber(w.aggregate_delta)} but every segment moved the opposite way.`;
    case "opposing_offset":
      return `Opposing offsets: ${w.component_a} ${
        w.delta_a >= 0 ? "+" : ""
      }${formatNumber(w.delta_a)} cancels with ${w.component_b} ${
        w.delta_b >= 0 ? "+" : ""
      }${formatNumber(w.delta_b)} — the net move hides a bigger shift in both components.`;
    case "non_additive_dimension_split":
      return `${w.measure} is a ${w.measure_type} measure — per-element deltas on ${w.dimension} don't sum to the parent delta, so concentrations are approximations.`;
  }
}

function formatNumber(n: number): string {
  if (Math.abs(n) >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (Math.abs(n) >= 1_000) return `${(n / 1_000).toFixed(2)}k`;
  return n.toFixed(2);
}
