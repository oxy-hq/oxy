import type { NodeProps } from "@xyflow/react";
import { cn } from "@/libs/shadcn/utils";
import { GraphNodeCard, GraphNodeHandles } from "../../components/semanticGraph";
import { measureDescription, measureTitle } from "../measureTitle";
import { SCENARIO_NODE_PRESENTATION, type ScenarioNodeData } from "./nodeValue";
import { ScenarioValueRow } from "./ScenarioValueRow";

export function ScenarioNode({ data }: NodeProps) {
  const scenarioData = data as unknown as ScenarioNodeData;
  const { node, state } = scenarioData;
  // A scenario state maps onto the shared card's three states rather than onto
  // a border colour of its own: the pinned lever is what the graph is focused
  // on, anything the scenario didn't move recedes, and an unreachable measure
  // is pushed furthest back. The edges read the same map — see `nodeValue`.
  const presentation = SCENARIO_NODE_PRESENTATION[state];

  return (
    <>
      <GraphNodeHandles />
      <GraphNodeCard {...presentation} data-testid={`scenario-node-${node.id}`}>
        <div className='flex items-baseline justify-between gap-1.5'>
          <span
            className={cn(
              "truncate font-medium text-[12px] leading-tight",
              presentation.dimmed ? "text-muted-foreground" : "text-foreground"
            )}
            title={measureDescription(node) ?? measureTitle(node)}
          >
            {measureTitle(node)}
          </span>
        </div>
        <ScenarioValueRow data={scenarioData} />
      </GraphNodeCard>
    </>
  );
}
