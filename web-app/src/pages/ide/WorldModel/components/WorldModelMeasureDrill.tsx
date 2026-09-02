import { ChevronDown, ChevronRight } from "lucide-react";
import { useState } from "react";
import type { OpportunityInstance } from "@/types/metricTree";
import { InfoTip } from "./panelPrimitives";
import { METHOD_HELP } from "./WorldModelDrillChain";
import { WorldModelSegmentDrill } from "./WorldModelSegmentDrill";

interface WorldModelMeasureDrillProps {
  /** Metric-tree node id (`view.measure`) being decomposed. */
  nodeId: string;
  /** Time dimension already resolved by the Opportunities section. */
  timeDimension: string;
  periodDays: number;
  /** Instance scope, when the panel is instance-scoped. */
  instance?: OpportunityInstance;
}

/**
 * The whole-measure decomposition, for measures that have no ranked rows to
 * expand.
 *
 * The Opportunities section only ranks `weight_basis: "rows"` responses — an
 * average or a `type: count` answers `equal`/`value_share`, whose gaps are rate
 * spreads with no amount attached, so ranking them under an upside heading
 * would promise a number nobody can go get. That gate is correct and stays. But
 * dropping the rows also dropped the only place a drill could hang off, so a
 * `type: count` measure — which decomposes perfectly well through the
 * value-share path — silently lost the affordance the deleted standalone Drill
 * section gave it. That is the same fail-closed capability loss recorded in
 * `internal-docs/world-model-opportunities.md` -> *Three predicates that are not
 * interchangeable*, and `drillable` being `false` for count/min/max by design is
 * exactly why the section's gate is the union `drillable || additive`.
 *
 * So: no ranked rows, one measure-level chain instead — rooted at the engine's
 * own top pick (no `root` field), which is precisely what the old section did.
 * Same expand-on-click discipline as a row: `WorldModelSegmentDrill` is mounted
 * only while open, so first paint still costs zero drill queries.
 */
export function WorldModelMeasureDrill({
  nodeId,
  timeDimension,
  periodDays,
  instance
}: WorldModelMeasureDrillProps) {
  const [open, setOpen] = useState(false);

  return (
    <div className='flex flex-col gap-1'>
      <div className='flex items-center gap-1'>
        <button
          type='button'
          onClick={() => setOpen((v) => !v)}
          aria-expanded={open}
          aria-label={`Decompose ${nodeId}`}
          data-testid={`wm-opp-measure-drill-toggle-${nodeId}`}
          className='flex items-center gap-1 text-left font-mono text-[9.5px] text-muted-foreground transition-colors hover:text-foreground'
        >
          {open ? <ChevronDown size={10} /> : <ChevronRight size={10} />}
          follow this measure's gap down, level by level
        </button>
        <InfoTip content={METHOD_HELP} />
      </div>
      {open && (
        <div className='pl-3'>
          <WorldModelSegmentDrill
            nodeId={nodeId}
            timeDimension={timeDimension}
            periodDays={periodDays}
            instance={instance}
          />
        </div>
      )}
    </div>
  );
}
