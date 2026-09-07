import { useState } from "react";
import type { FittedDriver, MetricTree } from "@/types/metricTree";
import { SectionHeader } from "../../components/semanticGraph";
import { ImpactRow } from "./ImpactRow";
import type { ScenarioNodeData } from "./nodeValue";
import type { ScenarioRunState } from "./useScenario";

interface ImpactListProps {
  /** The same per-node map the canvas renders, reused so the list and the
   *  canvas can never disagree about what moved. */
  nodeData: Map<string, ScenarioNodeData>;
  /** Focus a measure on the canvas — the list is how you find it. */
  onSelect: (nodeId: string) => void;
  /** Edge metadata for the expanded route. Optional so the list still renders
   *  from `nodeData` alone; without it a route is named but unannotated. */
  tree?: Pick<MetricTree, "edges">;
  /** The baseline's fitted coefficients, so an expanded hop can say whether its
   *  number was declared or measured. */
  fitted?: FittedDriver[];
  /** Pinned lever ids — only used to detect a measure several routes reach. */
  leverIds?: string[];
  /** Why an empty list is empty. Defaults to `ran`, i.e. "propagation
   *  happened and found nothing", which is the only one of the three that is
   *  a claim about the model. */
  runState?: ScenarioRunState;
}

/**
 * What the scenario actually moved, as a list.
 *
 * The canvas alone is not enough: a lever that moves one measure out of 266
 * leaves a single node changed somewhere off-screen, which reads as "nothing
 * happened". The count of unaffected measures cannot answer "so what DID
 * change?" — this can.
 *
 * A row also expands into `ImpactDetail`, which answers the question the number
 * alone provokes: on what basis. One row open at a time — the panel is a column
 * beside the canvas, and several open traces push the rest of the list out of
 * sight, which is the problem this list exists to solve.
 */
export function ImpactList({
  nodeData,
  onSelect,
  runState = "ran",
  tree,
  fitted,
  leverIds
}: ImpactListProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const impacts = [...nodeData.values()]
    .filter((d) => d.state === "impacted" || d.state === "unquantifiable")
    .sort((a, b) => Math.abs(b.delta ?? 0) - Math.abs(a.delta ?? 0));

  return (
    <div
      className='flex flex-col gap-2 border-border border-t p-4'
      data-testid='scenario-impact-list'
    >
      <SectionHeader
        title='Downstream impact'
        subtitle={impacts.length > 0 ? `${impacts.length}` : undefined}
      />

      {impacts.length === 0 && runState === "unresolved" ? (
        <p
          className='text-[11px] text-muted-foreground leading-relaxed'
          data-testid='scenario-nothing-simulated'
        >
          Nothing was simulated — no pinned lever's value could be resolved, so no propagation ran.
          This is not a claim that the lever moves nothing; fix the value above and it will run.
        </p>
      ) : impacts.length === 0 && runState === "unmoved" ? (
        // Distinct from both neighbours: nothing is wrong with the input, and
        // nothing has been simulated either. This is where a freshly pinned
        // lever lands, so the old two-way split showed it "this lever moves no
        // other modelled measure" — a modelling claim about a run that never
        // happened.
        <p
          className='text-[11px] text-muted-foreground leading-relaxed'
          data-testid='scenario-lever-unmoved'
        >
          Nothing to simulate yet — the lever is still at its current value. Type a new value, or
          drag the slider, and the impact will appear here.
        </p>
      ) : impacts.length === 0 ? (
        <p className='text-[11px] text-muted-foreground leading-relaxed'>
          This lever moves no other modelled measure. Only measures connected by a component
          expression or a <span className='font-mono'>drivers:</span> entry can propagate.
        </p>
      ) : (
        <ul className='flex flex-col gap-1'>
          {impacts.map((d) => {
            const expanded = expandedId === d.node.id;
            return (
              <ImpactRow
                key={d.node.id}
                data={d}
                expanded={expanded}
                onToggle={() => {
                  onSelect(d.node.id);
                  setExpandedId(expanded ? null : d.node.id);
                }}
                tree={tree}
                fitted={fitted}
                leverIds={leverIds}
              />
            );
          })}
        </ul>
      )}
    </div>
  );
}
