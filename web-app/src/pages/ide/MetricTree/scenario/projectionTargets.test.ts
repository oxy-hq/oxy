import { describe, expect, it } from "vitest";
import type { MetricNode } from "@/types/metricTree";
import type { ScenarioNodeData } from "./nodeValue";
import { projectionTargets, resolveTarget } from "./projectionTargets";

function node(id: string): MetricNode {
  return { id, measure: id.split(".")[1] ?? id, label: id } as MetricNode;
}

function data(id: string, over: Partial<ScenarioNodeData>): ScenarioNodeData {
  return { node: node(id), state: "impacted", ...over } as ScenarioNodeData;
}

function map(...entries: ScenarioNodeData[]): Map<string, ScenarioNodeData> {
  return new Map(entries.map((d) => [d.node.id, d]));
}

describe("projectionTargets", () => {
  it("puts levers first, then the biggest movers", () => {
    const targets = projectionTargets(
      map(
        data("v.small", { delta: 5 }),
        data("v.lever", { state: "lever", delta: 100 }),
        data("v.big", { delta: -900 })
      )
    );
    expect(targets.map((t) => t.nodeId)).toEqual(["v.lever", "v.big", "v.small"]);
    expect(targets[0]?.isLever).toBe(true);
  });

  it("labels a target by its measure name, not its description", () => {
    const targets = projectionTargets(map(data("checks.avg_check", { delta: 1 })));
    expect(targets[0]?.label).toBe("avg_check");
  });

  /** The model knows these moved and cannot size the move; the panel says so.
   *  Dropping them would make the picker narrower than the model. */
  it("offers unquantifiable measures", () => {
    const targets = projectionTargets(map(data("v.cant", { state: "unquantifiable" })));
    expect(targets.map((t) => t.nodeId)).toEqual(["v.cant"]);
  });

  /** A dropdown entry whose only possible outcome is "this lever doesn't move
   *  this measure" is a dropdown entry not worth having. */
  it("excludes measures the scenario never touched", () => {
    const targets = projectionTargets(
      map(
        data("v.unchanged", { state: "unchanged" }),
        data("v.unreachable", { state: "unreachable" }),
        data("v.unvalued", { state: "unvalued" })
      )
    );
    expect(targets).toEqual([]);
  });
});

describe("resolveTarget", () => {
  const targets = projectionTargets(
    map(data("v.lever", { state: "lever" }), data("v.moved", { delta: 3 }))
  );

  it("honours a valid choice", () => {
    expect(resolveTarget(targets, "v.moved")).toBe("v.moved");
  });

  /** Clicking an unaffected node while hunting for the next lever must not
   *  blank the chart. */
  it("falls back to the first target when the choice is not offerable", () => {
    expect(resolveTarget(targets, "v.elsewhere")).toBe("v.lever");
    expect(resolveTarget(targets, null)).toBe("v.lever");
  });

  it("has nothing to show when nothing moved", () => {
    expect(resolveTarget([], "v.moved")).toBeNull();
  });
});
