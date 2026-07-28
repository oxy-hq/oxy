import { ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useMetricTree, useOpportunityQuery, useTimeDimensions } from "@/hooks/api/useMetricTree";
import type { OpportunityInstance, OpportunityRequest } from "@/types/metricTree";
import type { WmSelection, WorldModel } from "@/types/worldModel";
import { InfoTip, SectionHeader, SectionSpinner } from "./panelPrimitives";
import { METHOD_HELP } from "./WorldModelDrillChain";
import { WorldModelMeasureDrill } from "./WorldModelMeasureDrill";
import { ScanControls } from "./WorldModelScanControls";
import { SizingBody } from "./WorldModelSegmentGroups";

export const DEFAULT_PRESET_DAYS = 90;

/**
 * Substring of airlayer's skip reason when an additive sum can't be sized for
 * lack of a `count` denominator to form per-unit rates. This is coupled to the
 * engine's message text because the crate exposes no structured reason code yet
 * — if it gains one (`reason_code`), match on that instead. Centralised here so
 * the coupling is in one obvious place.
 */
const NO_COUNT_REASON_HINT = "count";

/** `n` days before today, as `YYYY-MM-DD` (UTC). */
function daysAgoIso(n: number): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - n);
  return d.toISOString().slice(0, 10);
}

/**
 * Trailing window of `days`, ending yesterday. Today is excluded deliberately:
 * a partial day would read as a depressed segment — the same reason monitor
 * scans skip the incomplete period.
 */
export function presetPeriod(days: number): [string, string] {
  return [daysAgoIso(days), daysAgoIso(1)];
}

interface WorldModelOpportunitiesSectionProps {
  /** Metric-tree node id (`view.measure`). */
  nodeId: string;
  /** View the measure is declared on — where its time dimensions live. */
  view: string;
  /** `additive` | `non_additive` | `passthrough`, from the world-model graph. */
  additivity: string;
  /** Trailing-window preset, owned by the parent so it survives measure switches. */
  periodDays: number;
  onPeriodDaysChange: (days: number) => void;
  /** The graph, for resolving a dimension header back to a selectable node. */
  model: WorldModel;
  onSelect: (s: WmSelection) => void;
  /**
   * Scope the scan to one instance — "within this store, where is the upside?"
   * Omit to size across the whole population, which is what the measure panel
   * asks.
   *
   * Scoping changes which dimensions survive, by design: anything the instance
   * pins to a single value (a store's own name, city, region) has no peers left
   * to benchmark against and drops out, leaving the cuts that still vary inside
   * the instance.
   */
  instance?: OpportunityInstance;
  /**
   * Resolve the sizing query eagerly and render **nothing at all** until it
   * confirms there is something to size. Used in the instance panel, where an
   * "Opportunities" toggle that only opens onto "no data" is noise — better to
   * never show it. Costs one warehouse sizing scan per mounted measure up front
   * (5-min cached) instead of on first expand. Off by default: the
   * single-measure detail panel keeps the cheap lazy behavior and shows an
   * explanatory empty state instead.
   *
   * Note this compounds with `instance`: a scoped scan caches per instance
   * rather than once for the population, so the eager cost is paid per instance
   * visited rather than amortized across all of them.
   */
  hideWhenEmpty?: boolean;
}

/**
 * "Where is the upside, what would closing it add, and where does that gap come
 * from?" — the whole segment half of the improve-this-measure panel. It drives
 * the same metric-tree `opportunity` endpoint the analytics agent uses, and
 * renders the result deterministically (no LLM).
 *
 * This was two sibling sections until they merged. They were never two features:
 * the engine's `opportunity_drill` calls `opportunity()` and keeps only the top
 * row, so "Drill" was this section's #1 row decomposed, rendered as if it were
 * an independent finding. Now the ranked rows ARE drill level 0 — each expands
 * into its own chain (`WorldModelSegmentDrill`), so every cut is actionable
 * rather than only the engine's pick, which is merely badged `top pick`. Nothing
 * auto-expands: first paint costs one opportunity scan and zero drill queries.
 *
 * It shows exactly one thing: **addressable upside**. The engine only produces
 * that for an additive sum with a declared `count` denominator — it sizes each
 * segment on a per-unit RATE (total ÷ count), benchmarks the rate, and applies
 * the gap to the segment's own volume (`weight_basis: "rows"`), so a small
 * segment can't masquerade as headroom just for being small. If the view
 * declares no `count` measure the engine refuses to size, and we say so rather
 * than draw a size-confounded number.
 *
 * No ranked rows are drawn for any other mode. The engine will still answer for
 * an average (`weight_basis: "equal"`) or a count/min/max (`"value_share"`), but
 * those answers are rate spreads with no upside attached — a gap in the
 * measure's own units, not an amount anyone can go get. Ranking them under
 * "Opportunities · addressable upside" put a diagnostic where a number to act on
 * was promised, so they are not rendered at all; the drivers section above is
 * where a non-sum measure is reasoned about.
 */
export function WorldModelOpportunitiesSection({
  nodeId,
  view,
  additivity,
  periodDays,
  onPeriodDaysChange,
  model,
  onSelect,
  instance,
  hideWhenEmpty = false
}: WorldModelOpportunitiesSectionProps) {
  const [open, setOpen] = useState(false);
  const [timeDimOverride, setTimeDimOverride] = useState<string | null>(null);

  // Cached by every caller of this section (both parents call `useMetricTree()`
  // and wait for a resolved node before mounting it), so this reads the
  // query-client cache rather than firing a second request.
  const { data: tree } = useMetricTree();
  const node = useMemo(() => tree?.nodes.find((n) => n.id === nodeId), [tree, nodeId]);

  // Can the engine decompose a row of this measure at all? Two DIFFERENT
  // questions, and the affordance needs both (inherited verbatim from the old
  // standalone drill section's `canQuery`):
  //
  //  - `drillable` (airlayer's `supports_rate_basis`) asks "will the engine size
  //    this on a per-unit RATE?" — true for a plain sum or an eligible additive
  //    composite, false for `count`/`min`/`max` by design.
  //  - `additive` is the broader question: `opportunity()` still serves
  //    count/min/max through the value-share path. Gating on `drillable` alone
  //    made the affordance vanish for `type: count` measures — a fail-closed
  //    capability loss.
  //
  // Component-edge presence is NOT the engine's acceptance predicate: a
  // passthrough with `{{}}` refs gets a component edge whether or not the engine
  // accepts it, so gating on edges rendered plausible, silently wrong numbers.
  const drillEnabled = (node?.drillable ?? false) || additivity === "additive";

  // Worth a query? An additive measure can reach a sized result; so can a
  // drillable composite, which airlayer classifies `passthrough` even though the
  // engine sizes it fine. Narrowing this to `additive` alone would hide the
  // merged section for exactly the composites (`checks.net_revenue`) the drill
  // half exists to expose — the pre-merge code only got away with it because the
  // drill was a second, separately-gated section.
  const canQuery = additivity === "additive" || drillEnabled;

  // In hide-when-empty mode the query must resolve before the toggle can render,
  // so run it eagerly; otherwise stay lazy and only fetch once the user expands.
  const active = open || hideWhenEmpty;

  const { data: timeDims, isPending: timeDimsPending } = useTimeDimensions({
    enabled: active && canQuery
  });

  const candidates = useMemo<string[]>(() => timeDims?.by_view[view] ?? [], [timeDims, view]);
  const timeDim =
    timeDimOverride && candidates.includes(timeDimOverride)
      ? timeDimOverride
      : (candidates[0] ?? "");

  const request = useMemo<OpportunityRequest | null>(() => {
    if (!canQuery || !timeDim) return null;
    return {
      target: nodeId,
      time_dimension: timeDim,
      period: presetPeriod(periodDays),
      ...(instance ? { instance } : {})
    };
  }, [canQuery, nodeId, timeDim, periodDays, instance]);

  const opp = useOpportunityQuery(request, active && !!request);

  // The engine picks the sizing mode; the UI follows the response rather than
  // re-deriving it from additivity, so the two can never disagree. Anything but
  // "rows" carries no upside, so its dimensions are dropped rather than ranked
  // under a heading that promises one.
  const rowsMode = opp.data?.weight_basis === "rows";
  const dimensions = useMemo(
    () => (rowsMode ? (opp.data?.dimensions ?? []) : []),
    [opp.data, rowsMode]
  );
  // Sum-like measure the engine could not size because the view has no `count`
  // measure to form a per-unit rate.
  const refusedNoCount =
    canQuery &&
    !!opp.data &&
    rowsMode &&
    dimensions.length === 0 &&
    opp.data.skipped_dimensions.some((s) => s.reason.includes(NO_COUNT_REASON_HINT));

  // A non-rows answer the engine can still decompose (`type: count` and friends)
  // — no ranked rows, but a whole-measure chain rooted at the engine's own pick.
  const measureDrillOnly = drillEnabled && !!opp.data && !rowsMode && !refusedNoCount;

  // Is there anything worth a toggle? Sized upside, the *actionable* no-count
  // refusal, a measure-level decomposition, or an error we must not swallow. A
  // resolved-but-empty result with none of those is not content.
  const hasContent =
    canQuery &&
    ((!!opp.data && (dimensions.length > 0 || refusedNoCount || measureDrillOnly)) || !!opp.error);

  // Latch: once shown, stay mounted so changing the period (which briefly clears
  // `opp.data` while the new window loads, or lands on an empty window) doesn't
  // make the whole section vanish mid-exploration. Only the initial appearance
  // is gated on having content.
  const [everShown, setEverShown] = useState(false);
  useEffect(() => {
    if (hasContent) setEverShown(true);
  }, [hasContent]);

  // A non-additive or passthrough measure can never yield sized upside, and that
  // is known from its type alone — no query, no toggle, no section. This is the
  // one case we can hide for free; everything else has to ask the engine first.
  if (!canQuery) return null;

  // Instance panel: render nothing until the eager query confirms there is data
  // to size — no collapsed toggle that would only open onto "no data".
  if (hideWhenEmpty && !hasContent && !everShown) return null;

  return (
    <section className='flex flex-col gap-1.5 border-border border-t pt-3'>
      <button
        type='button'
        onClick={() => setOpen((v) => !v)}
        className='flex w-full items-center gap-1 text-left'
        aria-expanded={open}
        data-testid={`wm-opp-toggle-${nodeId}`}
      >
        {open ? (
          <ChevronDown size={12} className='shrink-0 text-muted-foreground' />
        ) : (
          <ChevronRight size={12} className='shrink-0 text-muted-foreground' />
        )}
        <span className='min-w-0 flex-1'>
          <SectionHeader
            title='Opportunities'
            subtitle='addressable upside · and where it comes from'
          />
        </span>
      </button>

      {open && (
        <div className='flex flex-col gap-2 pt-1'>
          <ScanControls
            view={view}
            periodDays={periodDays}
            onPeriodDaysChange={onPeriodDaysChange}
            timeDim={timeDim}
            candidates={candidates}
            onTimeDimChange={setTimeDimOverride}
          />

          {/* The merged section's second half, stated once at the top rather
              than repeated on every row: each ranked row IS drill level 0, and
              expanding it follows that row's own gap down.

              Gated on there actually BEING rows, not merely on `drillEnabled`:
              a measure that lands on "No addressable upside to size" has no row
              to expand, and telling the reader to expand one sends them looking
              for an affordance that isn't there. That case gets its own
              measure-level chain below, which carries its own hint. */}
          {drillEnabled && dimensions.length > 0 && (
            <p className='font-mono text-[9.5px] text-muted-foreground'>
              expand a row to follow its gap down, level by level <InfoTip content={METHOD_HELP} />
            </p>
          )}

          {timeDimsPending && <SectionSpinner />}

          {!timeDimsPending && !request && (
            <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
              No date or datetime dimension is declared on {view}, so there is no period to compare
              over.
            </p>
          )}

          {request && opp.isPending && <SectionSpinner label='scanning warehouse segments…' />}

          {request && opp.error && (
            <p className='font-mono text-[10px] text-destructive leading-relaxed'>
              {opp.error instanceof Error ? opp.error.message : "Failed to size segments."}
            </p>
          )}

          {refusedNoCount && (
            <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
              Can't size a total fairly without a volume to normalize it: {view} declares no{" "}
              <span className='text-foreground'>count</span> measure, so segment totals can't be
              turned into comparable per-unit rates. Add a{" "}
              <span className='text-foreground'>type: count</span> measure to size this.
            </p>
          )}

          {/* The engine answered, but in a mode that carries no upside (an
              average, or a count/min/max). Say that, rather than let the
              no-spread message below imply the data was flat — the dimensions
              may well vary; there is simply no amount to go get. */}
          {request && opp.data && !refusedNoCount && !rowsMode && (
            <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
              No addressable upside to size: only a total divided by a count gives a rate whose gap
              is worth an amount. Use the drivers above.
            </p>
          )}

          {/* …but "no upside to size" is not "nothing to decompose". A
              `type: count` answers `value_share`, which the engine still drills
              — it just has no rate denominator. With no ranked rows to hang a
              per-row chain off, offer the whole-measure one instead, rooted at
              the engine's own top pick. Lazy, like a row: nothing fetches until
              this is expanded. */}
          {request && measureDrillOnly && (
            <WorldModelMeasureDrill
              nodeId={nodeId}
              timeDimension={timeDim}
              periodDays={periodDays}
              instance={instance}
            />
          )}

          {request && opp.data && !refusedNoCount && rowsMode && dimensions.length === 0 && (
            <p className='font-mono text-[10px] text-muted-foreground leading-relaxed'>
              No dimension had enough spread to size
              {opp.data.skipped_dimensions.length > 0
                ? ` (${opp.data.skipped_dimensions[0].reason})`
                : ""}
              .
            </p>
          )}

          {request && opp.data && dimensions.length > 0 && (
            <SizingBody
              dimensions={dimensions}
              target={nodeId}
              view={view}
              periodDays={periodDays}
              overallValue={opp.data.overall_value}
              rateDenominator={opp.data.rate_denominator}
              scope={instance}
              model={model}
              onSelect={onSelect}
              timeDimension={timeDim}
              instance={instance}
              drillEnabled={drillEnabled}
            />
          )}
        </div>
      )}
    </section>
  );
}
