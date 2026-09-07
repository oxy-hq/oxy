import { measureTitle } from "../measureTitle";
import type { ScenarioNodeData } from "./nodeValue";

/**
 * The measures worth offering a forward curve for, in the order to offer them.
 *
 * Levers first, then whatever they moved, biggest move first — the same
 * ordering the impact list uses, so the picker and the list beneath it agree
 * about what matters. A measure the scenario did not touch is excluded: its
 * baseline curve would draw fine and its scenario curve would be refused
 * `unmoved` every time, which is a dropdown entry whose only outcome is a
 * shrug.
 *
 * `unquantifiable` measures ARE offered. The model knows they moved and cannot
 * size the move; the panel says so in words, and leaving them out would make
 * the surface quietly narrower than the model.
 */
export interface ProjectionTarget {
  nodeId: string;
  label: string;
  isLever: boolean;
}

export function projectionTargets(nodeData: Map<string, ScenarioNodeData>): ProjectionTarget[] {
  const levers: ProjectionTarget[] = [];
  const moved: (ProjectionTarget & { magnitude: number })[] = [];

  for (const data of nodeData.values()) {
    const target = {
      nodeId: data.node.id,
      label: measureTitle(data.node),
      isLever: data.state === "lever"
    };
    if (data.state === "lever") levers.push(target);
    else if (data.state === "impacted" || data.state === "unquantifiable") {
      moved.push({ ...target, magnitude: Math.abs(data.delta ?? 0) });
    }
  }

  moved.sort((a, b) => b.magnitude - a.magnitude);
  return [...levers, ...moved.map(({ magnitude: _magnitude, ...rest }) => rest)];
}

/**
 * Which target the chart should show, given what the analyst last clicked.
 *
 * A canvas selection wins **only when it is actually offerable** — clicking an
 * unaffected node elsewhere on the graph must not blank the chart, since the
 * canvas is also how you go looking for the next lever.
 */
export function resolveTarget(targets: ProjectionTarget[], chosen: string | null): string | null {
  if (chosen && targets.some((t) => t.nodeId === chosen)) return chosen;
  return targets[0]?.nodeId ?? null;
}
