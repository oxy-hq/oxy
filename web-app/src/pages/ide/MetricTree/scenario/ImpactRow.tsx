import { ChevronRight } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { FittedDriver, MetricTree } from "@/types/metricTree";
import { formatNumber } from "@/utils/measureFormat";
import { Row } from "../../components/semanticGraph";
import { measureTitle } from "../measureTitle";
import { ConfidenceMark } from "./ConfidenceMark";
import { ImpactDetail } from "./ImpactDetail";
import { MeasureChange } from "./MeasureChange";
import type { ScenarioNodeData } from "./nodeValue";

interface ImpactRowProps {
  data: ScenarioNodeData;
  expanded: boolean;
  /** Focus the measure on the canvas and toggle this row open. One gesture:
   *  the click already meant "this one", and splitting it in two would leave
   *  the old dead-click behaviour as one of them. */
  onToggle: () => void;
  tree?: Pick<MetricTree, "edges">;
  fitted?: FittedDriver[];
  leverIds?: string[];
}

/**
 * One impacted measure, and — when opened — the basis for its number.
 *
 * Its own component because it is a distinct named concept with its own
 * interaction: the header is the only route into an impact's provenance, and
 * that makes its markup load-bearing rather than incidental to the list.
 */
export function ImpactRow({ data, expanded, onToggle, tree, fitted, leverIds }: ImpactRowProps) {
  return (
    <li className='flex flex-col'>
      <Row className='p-0'>
        {/* A real button, not a clickable Row: this is the surface's only route
            into an impact's basis, and a div with an onClick puts it out of
            reach of the keyboard entirely. */}
        <button
          type='button'
          className='flex min-w-0 flex-1 items-center justify-between gap-2 px-2 py-1.5 text-left'
          aria-expanded={expanded}
          onClick={onToggle}
          data-testid={`scenario-impact-row-${data.node.id}`}
        >
          <span className='flex min-w-0 items-center gap-1'>
            <ChevronRight
              size={11}
              className={cn(
                "shrink-0 text-muted-foreground transition-transform",
                expanded && "rotate-90"
              )}
            />
            <span className='min-w-0 truncate text-foreground' title={data.node.id}>
              {measureTitle(data.node)}
            </span>
          </span>
          <ImpactValue data={data} />
        </button>
      </Row>
      {expanded && <ImpactDetail data={data} tree={tree} fitted={fitted} leverIds={leverIds} />}
    </li>
  );
}

function ImpactValue({ data }: { data: ScenarioNodeData }) {
  const { state, baseline, simulated, delta, confidence } = data;

  // `estimated_delta` is 0.0 when unquantifiable, meaning UNKNOWN. Showing a
  // number here would be the list telling the same lie the canvas refuses to.
  if (state === "unquantifiable") {
    return <ConfidenceMark confidence='unquantifiable' withLabel />;
  }

  // Spelled out here, glyph-only on the canvas: this is the surface with room
  // for the word, so it is where the glyph gets learned.
  return (
    <span className='shrink-0'>
      <MeasureChange
        baseline={baseline}
        simulated={simulated}
        delta={delta}
        // `formatNumber`, not the canvas's compact `formatValue`: this is the
        // side panel, and it sits in the same rail as `LeverList`. Two
        // formatters there rendered the SAME measure two ways a few rows
        // apart — `MeasureChange`'s own prop doc names `formatNumber` as the
        // panel's.
        format={formatNumber}
        confidence={confidence}
        confidenceLabel
      />
    </span>
  );
}
