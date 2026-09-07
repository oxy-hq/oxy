import { ChevronDown, ChevronRight, Copy } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import type { OpportunityInstance, SegmentOpportunity } from "@/types/metricTree";
import { MagnitudeBar, MetaBadge } from "../../components/semanticGraph";
import { formatCompact, formatCount, formatDelta, formatSignedPct } from "./measureTarget";
import { WorldModelSegmentDrill } from "./WorldModelSegmentDrill";
import { formatSegment, segmentQuestion } from "./worldModelNav";

/** Why one row is badged when every row is expandable: the engine still has an
 *  opinion, and hiding it would lose the "follow the max" recommendation the
 *  single-path drill used to be. Marked, not privileged — nothing auto-opens. */
const TOP_PICK_HELP = "The engine's own top pick — the row a single-path drill would follow.";

/** Props every ranked row needs to root its own decomposition. Grouped rather
 *  than passed one by one: they thread unchanged through three components. */
export interface RowDrillContext {
  /** Metric-tree node id (`view.measure`) the rows were sized against. */
  nodeId: string;
  /** Time dimension the section already resolved — the rows reuse it. */
  timeDimension: string;
  /** Instance scope, when the panel is instance-scoped. */
  instance?: OpportunityInstance;
  /** Whether the engine can decompose this measure at all. Decided by the
   *  section (`drillable || additive`); a row must never offer a chain the
   *  engine would refuse. */
  drillEnabled: boolean;
}

async function copyQuestion(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    toast.success("Question copied — paste it into Ask Oxygen.");
  } catch (err) {
    console.error("clipboard write failed", err);
    toast.error("Couldn't copy the question to the clipboard.");
  }
}

/**
 * A single sized segment: upside, a proportion bar, rate-vs-benchmark, copy —
 * and, when the measure can be decomposed, its own drill chain rooted at this
 * row. Expansion state is local to the row, so opening one row neither collapses
 * another nor costs anything on the rows left closed: `WorldModelSegmentDrill`
 * is mounted only while open, and it owns the query.
 */
export function SizedSegmentRow({
  seg,
  dimension,
  target,
  periodDays,
  maxUpside,
  overallValue,
  unit,
  scope,
  drill,
  isTopPick
}: {
  seg: SegmentOpportunity;
  dimension: string;
  target: string;
  periodDays: number;
  maxUpside: number;
  overallValue: number;
  /** Noun for what the rate is per, and what `volume` counts. */
  unit: { one: string; many: string };
  scope?: OpportunityInstance;
  drill: RowDrillContext;
  /** The row an unrooted drill would have followed. Badged, never auto-opened. */
  isTopPick: boolean;
}) {
  const [drillOpen, setDrillOpen] = useState(false);
  const pct = formatSignedPct(seg.upside, overallValue);
  return (
    <div className='flex flex-col gap-0.5' data-testid={`wm-opp-seg-${seg.segment}`}>
      <div className='flex min-w-0 items-center justify-between gap-2 font-mono text-xs'>
        {drill.drillEnabled && (
          <button
            type='button'
            onClick={() => setDrillOpen((v) => !v)}
            aria-expanded={drillOpen}
            title={`Where does ${formatSegment(seg.segment)}'s gap come from?`}
            aria-label={`Decompose ${formatSegment(seg.segment)}`}
            data-testid={`wm-opp-drill-toggle-${seg.segment}`}
            className='shrink-0 text-muted-foreground transition-colors hover:text-foreground'
          >
            {drillOpen ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </button>
        )}
        <span className='min-w-0 flex-1 truncate text-foreground'>
          {formatSegment(seg.segment)}
        </span>
        {drill.drillEnabled && isTopPick && <MetaBadge tooltip={TOP_PICK_HELP}>top pick</MetaBadge>}
        <span className='flex shrink-0 items-center gap-1.5'>
          <span className='text-info'>{formatDelta(seg.upside)}</span>
          {pct && <span className='text-[9.5px] text-muted-foreground'>{pct}</span>}
          <button
            type='button'
            title='Copy a question to investigate this segment'
            aria-label='Copy a question to investigate this segment'
            data-testid={`wm-opp-ask-${seg.segment}`}
            onClick={() =>
              copyQuestion(
                segmentQuestion({
                  target,
                  dimension,
                  segment: seg.segment,
                  currentRate: seg.current_value,
                  benchmark: seg.benchmark,
                  upside: seg.upside,
                  periodDays,
                  scope
                })
              )
            }
            className='shrink-0 text-muted-foreground transition-colors hover:text-foreground'
          >
            <Copy size={11} />
          </button>
        </span>
      </div>
      <MagnitudeBar fraction={maxUpside > 0 ? seg.upside / maxUpside : 0} />
      {/* The whole sum, in the reader's units: the two rates are per-`unit`, the
          volume is a count of them, and their product is the upside above. Left
          as "rate 533.9 vs 801.6 · 189 rows" it was three unlabelled numbers
          that happened to multiply out to a figure nobody could reconstruct. */}
      <div className='flex justify-between pl-2 font-mono text-[9.5px] text-muted-foreground'>
        <span>
          {formatCompact(seg.current_value)} → {formatCompact(seg.benchmark)} per {unit.one}
        </span>
        <span>
          {formatCount(seg.volume)} {seg.volume === 1 ? unit.one : unit.many}
        </span>
      </div>
      {/* Level 1 and below of THIS row's chain. Mounted on expand only — the
          recursive scan is a bounded but real number of warehouse queries. */}
      {drillOpen && (
        <div className='pl-2'>
          <WorldModelSegmentDrill
            nodeId={drill.nodeId}
            timeDimension={drill.timeDimension}
            periodDays={periodDays}
            root={{ dimension, segment: seg.segment }}
            instance={drill.instance}
          />
        </div>
      )}
    </div>
  );
}
