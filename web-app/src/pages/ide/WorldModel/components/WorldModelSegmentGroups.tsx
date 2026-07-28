import { ArrowUpRight, ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { DimensionOpportunity, OpportunityInstance } from "@/types/metricTree";
import type { WmSelection, WorldModel } from "@/types/worldModel";
import { formatDelta, formatSignedPct, rowUnit } from "./measureTarget";
import { InfoTip, MetaBadge } from "./panelPrimitives";
import { type RowDrillContext, SizedSegmentRow } from "./WorldModelSizedSegmentRow";
import { dimensionNodeSelection } from "./worldModelNav";

const BEST_PEER_HELP =
  "Benchmark is the single top segment — it overfits noise. Prefer a p75 benchmark where the dimension has enough segments.";

/** Segments left out of a dimension's rows — but NOT out of its header total. */
const OMITTED_SEGMENTS_HELP =
  "Segments under 1% of this dimension's upside, plus any past the top-5 cap — the latter need not be small. The dimension's total above includes them, so it can exceed the rows shown here.";

/** The method note. Takes the denominator so the rate can be defined exactly
 *  rather than left as the bare word "rate".
 *
 *  The polarity sentence is stated even though no shipped measure violates it:
 *  both benchmark bases (`best_peer`, `p75`) are top-END rates, so "below the
 *  benchmark" is hard-wired to mean "headroom". That is right for revenue and
 *  profit and inverted for a cost, and nothing in the sizing path detects which
 *  it has — a cost sum would rank the CHEAPEST segment as the biggest
 *  opportunity. Today that can't be reached (the engine refuses a sum with no
 *  `count` denominator, and the cost views declare none), but the assumption is
 *  in the arithmetic either way, so the reader is told rather than left to infer
 *  it from a number that looks fine. */
function sizingMethodHelp(rateDenominator?: string | null): string {
  const rate = rateDenominator
    ? `Rate is the measure divided by ${rateDenominator}, evaluated within each segment. `
    : "";
  return `${rate}Upside sizes each segment's per-unit rate gap to the benchmark, applied to its own volume — not a raw total-to-total gap, so a smaller segment isn't mistaken for headroom. Only gaps large enough to outrank sampling noise are sized; the rest are reported as dropped rather than shown. Sizing assumes HIGHER IS BETTER: the benchmark is a top-end rate, so a segment below it reads as upside. On a cost-like measure that inverts — the cheapest segment would be ranked the biggest opportunity. Levers OVERLAP — each is a different cut of the same rows, so they cannot be added together. A gap is a question, not a forecast: confirm the segment is comparable before acting.`;
}

/** A benchmark-basis chip with the best-peer caveat on hover. */
function BenchmarkBadge({ basis }: { basis: string }) {
  return (
    <span className='flex items-center gap-1 font-mono text-[9.5px] text-muted-foreground'>
      benchmark
      <MetaBadge tooltip={basis === "best_peer" ? BEST_PEER_HELP : undefined}>
        {basis.replace("_", " ")}
      </MetaBadge>
    </span>
  );
}

/** Collapsible header shared by both group kinds: toggle + optional node jump. */
function GroupHeader({
  dim,
  open,
  onToggle,
  testId,
  trailing,
  onOpenNode
}: {
  dim: DimensionOpportunity;
  open: boolean;
  onToggle: () => void;
  testId: string;
  trailing: React.ReactNode;
  onOpenNode?: () => void;
}) {
  return (
    <div className='flex items-center gap-1'>
      <button
        type='button'
        onClick={onToggle}
        aria-expanded={open}
        data-testid={testId}
        className='flex min-w-0 flex-1 items-center gap-2 border border-border bg-background/40 px-2 py-1.5 text-left font-mono text-xs transition-colors hover:border-info/60'
      >
        {open ? (
          <ChevronDown size={11} className='shrink-0 text-muted-foreground' />
        ) : (
          <ChevronRight size={11} className='shrink-0 text-muted-foreground' />
        )}
        <span className='min-w-0 flex-1 truncate text-foreground'>{dim.dimension}</span>
        {trailing}
      </button>
      {onOpenNode && (
        <button
          type='button'
          onClick={onOpenNode}
          title={`Open ${dim.dimension}`}
          aria-label={`Open ${dim.dimension}`}
          className='shrink-0 p-1 text-muted-foreground transition-colors hover:text-foreground'
        >
          <ArrowUpRight size={12} />
        </button>
      )}
    </div>
  );
}

/** Additive-sum sizing (`weight_basis: "rows"`): ranked levers with upside. */
export function SizingBody({
  dimensions,
  target,
  view,
  periodDays,
  overallValue,
  rateDenominator,
  scope,
  model,
  onSelect,
  timeDimension,
  instance,
  drillEnabled
}: {
  dimensions: DimensionOpportunity[];
  /** Metric-tree node id (`view.measure`) the rows were sized against — the same
   *  id a row's drill roots itself on, so it is passed once under one name. */
  target: string;
  view: string;
  periodDays: number;
  /** The measure's overall value over the period — the denominator that turns an
   *  absolute upside into a share of the whole ("+8%"). */
  overallValue: number;
  /** `view.measure` id the rates were formed against, for the method note. */
  rateDenominator?: string | null;
  /** The instance the scan was narrowed to, if any. Carried down so a copied
   *  question states the same scope the numbers were measured in. */
  scope?: OpportunityInstance;
  model: WorldModel;
  onSelect: (s: WmSelection) => void;
} & Omit<RowDrillContext, "nodeId">) {
  const drill: RowDrillContext = { nodeId: target, timeDimension, instance, drillEnabled };
  const top = dimensions[0];
  const topPct = formatSignedPct(top.total_upside, overallValue);
  // The total covers only the segments that cleared the significance gate, so
  // "each below-benchmark segment" would claim a scope the number doesn't have
  // — and says so directly above a line admitting others were dropped.
  const someUnproven = top.segments_dropped_as_noise > 0;
  // "its peer rate" is only true of a best-peer benchmark. A p75 benchmark is an
  // interpolated percentile that no peer need actually have, so promising a
  // reader they could "reach a peer's rate" names a target that may not exist.
  const benchmarkPhrase =
    top.benchmark_basis === "p75" ? "reached its peers' p75 rate" : "reached its best peer's rate";
  return (
    <>
      <p className='font-mono text-[10px] leading-relaxed'>
        <span className='text-muted-foreground'>Biggest lever: </span>
        <span className='text-foreground'>{top.dimension}</span>
        <span className='text-info'> {formatDelta(top.total_upside)}</span>
        {/* Name the denominator. The measure's headline above this panel is its
            value over ALL time, while every percent here is a share of the
            selected period — so a reader checking the obvious way (upside ÷ the
            big number up there) gets a different answer and concludes the panel
            is broken. */}
        {topPct && (
          <span className='text-muted-foreground'>
            {" "}
            ({topPct} of {periodDays}d)
          </span>
        )}
        <span className='text-muted-foreground'>
          {" "}
          if{" "}
          {someUnproven ? "each segment with a provable shortfall" : "each below-benchmark segment"}{" "}
          {benchmarkPhrase}.
          {/* Only warn when there is something to add up. A caution that fires
              where it cannot apply is how readers learn to skip the cautions. */}
          {dimensions.length > 1 && " Levers overlap — don't add them."}{" "}
        </span>
        <InfoTip content={sizingMethodHelp(rateDenominator)} />
      </p>
      {dimensions.map((d, i) => (
        <SizingGroup
          key={d.dimension}
          dim={d}
          target={target}
          view={view}
          periodDays={periodDays}
          overallValue={overallValue}
          scope={scope}
          model={model}
          onSelect={onSelect}
          drill={drill}
          /* The first segment of the first dimension is the row an unrooted
             drill would have followed — the engine's own pick. */
          isTopDimension={i === 0}
        />
      ))}
    </>
  );
}

/** One dimension's ranked segments with the addressable upside of each. */
function SizingGroup({
  dim,
  target,
  view,
  periodDays,
  overallValue,
  scope,
  model,
  onSelect,
  drill,
  isTopDimension
}: {
  dim: DimensionOpportunity;
  target: string;
  view: string;
  periodDays: number;
  overallValue: number;
  scope?: OpportunityInstance;
  model: WorldModel;
  onSelect: (s: WmSelection) => void;
  drill: RowDrillContext;
  isTopDimension: boolean;
}) {
  const [open, setOpen] = useState(false);
  const node = dimensionNodeSelection(model, view, dim.dimension);
  const maxUpside = Math.max(0, ...dim.segments.map((s) => s.upside));
  const totalPct = formatSignedPct(dim.total_upside, overallValue);
  const unit = rowUnit(view);

  return (
    <div className='flex flex-col gap-1'>
      <GroupHeader
        dim={dim}
        open={open}
        onToggle={() => setOpen((v) => !v)}
        testId={`wm-opp-dim-${dim.dimension}`}
        trailing={
          <span className='flex shrink-0 items-baseline gap-1'>
            <span className='text-info'>{formatDelta(dim.total_upside)}</span>
            {totalPct && <span className='text-[9.5px] text-muted-foreground'>{totalPct}</span>}
          </span>
        }
        onOpenNode={node ? () => onSelect(node) : undefined}
      />
      {open && (
        <div className='flex flex-col gap-1 pl-3'>
          <div className='flex items-center gap-1'>
            <BenchmarkBadge basis={dim.benchmark_basis} />
            <span className='font-mono text-[9.5px] text-muted-foreground'>
              · rate per {unit.one} · % of {periodDays}d total
            </span>
          </div>
          {dim.segments.map((s, i) => (
            <SizedSegmentRow
              key={s.segment}
              seg={s}
              dimension={dim.dimension}
              target={target}
              periodDays={periodDays}
              maxUpside={maxUpside}
              overallValue={overallValue}
              unit={unit}
              scope={scope}
              drill={drill}
              isTopPick={isTopDimension && i === 0}
            />
          ))}
          {/* Not "lower-upside segments": the cut is top-5 as well as tail, and a
              segment past the cap need not be small. Calling them all minor
              undersells what's hidden, under a total that still counts them. */}
          {dim.other_segments_skipped > 0 && (
            <div className='flex items-center gap-1 pl-2 font-mono text-[9.5px] text-muted-foreground'>
              +{dim.other_segments_skipped} more segment
              {dim.other_segments_skipped === 1 ? "" : "s"} not shown
              <InfoTip content={OMITTED_SEGMENTS_HELP} />
            </div>
          )}
          {/* Segments that WERE below the benchmark but whose gap couldn't be
              told from noise. Showing the count keeps "nothing was wrong here"
              distinct from "we couldn't prove what was" — without it, a
              dimension left with one lever reads as a cleaner result than it is. */}
          {dim.segments_dropped_as_noise > 0 && (
            <div className='pl-2 font-mono text-[9.5px] text-muted-foreground'>
              {dim.segments_dropped_as_noise} below benchmark but within sampling noise — not sized
            </div>
          )}
        </div>
      )}
    </div>
  );
}
