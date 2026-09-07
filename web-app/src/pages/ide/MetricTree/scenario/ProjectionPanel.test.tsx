// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { MetricNode, ProjectionResponse } from "@/types/metricTree";
import type { ScenarioNodeData, ScenarioNodeState } from "./nodeValue";
import { ProjectionPanel } from "./ProjectionPanel";
import type { ScenarioState } from "./scenarioUrl";

// The panel owns a `useProjection` query, and the query needs the IDE's
// project/branch context, so the whole hook is stubbed.
const query = vi.hoisted(() => ({
  result: { data: undefined, isFetching: false, error: null } as {
    data: ProjectionResponse | undefined;
    isFetching: boolean;
    error: Error | null;
  }
}));

vi.mock("@/hooks/api/useMetricTree", () => ({
  useProjection: () => query.result
}));

// Radix's Select reaches for pointer capture and scrollIntoView, neither of
// which jsdom implements; without them opening the listbox throws.
beforeAll(() => {
  Element.prototype.hasPointerCapture ??= () => false;
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.scrollIntoView ??= () => {};
});

afterEach(() => {
  cleanup();
  query.result = { data: undefined, isFetching: false, error: null };
});

const state: ScenarioState = {
  levers: [{ nodeId: "stores.net_sales", raw: "+5%" }],
  periodDays: 90,
  timeDimension: "stores.business_date",
  instance: null
};

function node(id: string, nodeState: ScenarioNodeState, delta: number): ScenarioNodeData {
  return {
    node: { id, measure: id, label: id } as MetricNode,
    state: nodeState,
    baseline: 100,
    delta
  };
}

const LEVER = "stores.net_sales";
const IMPACTED = "stores.store_profit";
const OTHER = "stores.labor_cost";

/** A fresh `Map` every call — the identity churn is the whole subject here:
 *  `nodeData` is rebuilt on every `predict` response, ~300ms after a lever
 *  nudge. */
function nodeData(ids: string[] = [LEVER, IMPACTED, OTHER]): Map<string, ScenarioNodeData> {
  const all: Record<string, ScenarioNodeData> = {
    [LEVER]: node(LEVER, "lever", 10),
    [IMPACTED]: node(IMPACTED, "impacted", 8),
    [OTHER]: node(OTHER, "impacted", 3)
  };
  return new Map(ids.map((id) => [id, all[id] as ScenarioNodeData]));
}

function renderPanel(selectedId: string | null, data = nodeData()) {
  const view = render(
    <ProjectionPanel state={state} nodeData={data} selectedId={selectedId} blocked={false} />
  );
  // Everything below the header is behind the disclosure — the curve is a
  // second warehouse query, so nothing mounts until it is opened.
  fireEvent.click(screen.getByTestId("metric-tree-projection-toggle"));
  return {
    rerender: (nextSelectedId: string | null, nextData: Map<string, ScenarioNodeData>) =>
      view.rerender(
        <ProjectionPanel
          state={state}
          nodeData={nextData}
          selectedId={nextSelectedId}
          blocked={false}
        />
      )
  };
}

/** What the measure picker is currently pointed at. */
function measure(): string {
  return screen.getByTestId("projection-measure").textContent ?? "";
}

/** Pick a measure from the dropdown the way an analyst does. */
function chooseMeasure(id: string) {
  fireEvent.keyDown(screen.getByTestId("projection-measure"), { key: "ArrowDown" });
  const option = screen.getAllByRole("option").find((o) => o.textContent?.startsWith(id));
  if (!option) throw new Error(`no option for ${id}`);
  fireEvent.click(option);
}

describe("ProjectionPanel measure picker", () => {
  it("seeds the picker from the canvas selection", () => {
    renderPanel(IMPACTED);

    expect(measure()).toContain(IMPACTED);
  });

  /**
   * The regression this file exists for. `targets` is a memo over `nodeData`,
   * which is a brand-new `Map` on every `predict` response — so an effect that
   * lists `targets` as a dependency re-fires ~300ms after every lever nudge and
   * threw away whatever the analyst had picked from the dropdown. Scoping the
   * effect to "something the picker offers" did not help: the selected node is
   * offered, which is exactly why the sync ran.
   */
  it("keeps a manual choice when a new predict response rebuilds the targets", () => {
    const { rerender } = renderPanel(LEVER);
    chooseMeasure(IMPACTED);
    expect(measure()).toContain(IMPACTED);

    // Same selection, same measures, new Map — a debounced predict landing.
    rerender(LEVER, nodeData());

    expect(measure()).toContain(IMPACTED);
  });

  it("moves the picker when the canvas selection genuinely changes", () => {
    const { rerender } = renderPanel(LEVER);
    chooseMeasure(IMPACTED);

    rerender(OTHER, nodeData());

    expect(measure()).toContain(OTHER);
  });

  /** Clicking a node the scenario never touched must not blank the chart —
   *  the canvas is also how the next lever gets found. */
  it("ignores a canvas selection the picker does not offer", () => {
    const { rerender } = renderPanel(LEVER);
    chooseMeasure(IMPACTED);

    rerender("stores.unreached", nodeData());

    expect(measure()).toContain(IMPACTED);
  });

  /** The other half of the intent: a choice that stops being offerable falls
   *  back to a valid target rather than leaving the picker pointed at nothing. */
  it("falls back when the chosen measure drops out of the targets", () => {
    const { rerender } = renderPanel(LEVER);
    chooseMeasure(IMPACTED);

    rerender(LEVER, nodeData([LEVER, OTHER]));

    expect(measure()).toContain(LEVER);
  });
});
