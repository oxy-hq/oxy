// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OpportunityInstance } from "@/types/metricTree";
import type { WorldModel } from "@/types/worldModel";
import { WorldModelInstanceOpportunity } from "./WorldModelInstanceOpportunity";

// The merged section reaches the API only through these hooks, so mocking them
// here covers every request it can make — no QueryClientProvider needed.
const useMetricTree = vi.hoisted(() => vi.fn());
const useOpportunityQuery = vi.hoisted(() => vi.fn());
const useTimeDimensions = vi.hoisted(() => vi.fn());
const useDrillQuery = vi.hoisted(() => vi.fn());

vi.mock("@/hooks/api/useMetricTree", () => ({
  useMetricTree,
  useOpportunityQuery,
  useTimeDimensions,
  useDrillQuery
}));

const EMPTY_MODEL: WorldModel = { entities: [], edges: [] };
const INSTANCE: OpportunityInstance = { entity: "store", key: "S1" };

/** A sized (rows-mode) response, so the eager `hideWhenEmpty` gate lets the
 *  toggle render. `undefined` models "the engine had nothing to size". */
const SIZED = {
  target: "checks.net_revenue",
  period: ["2026-04-15", "2026-07-13"],
  overall_value: 100000,
  weight_basis: "rows",
  skipped_dimensions: [],
  downstream: [],
  dimensions: [
    {
      dimension: "checks.channel",
      cardinality: 4,
      benchmark_basis: "p75",
      total_upside: 12000,
      other_segments_skipped: 0,
      segments_dropped_as_noise: 0,
      segments: [
        {
          segment: "mobile_app",
          current_value: 21,
          volume: 400,
          benchmark: 30,
          gap: 9,
          upside: 3600
        }
      ]
    }
  ]
};

function renderInstanceOpportunity(additivity: string, drillable: boolean, opp: unknown = SIZED) {
  useMetricTree.mockReturnValue({
    data: {
      nodes: [
        {
          id: "checks.net_revenue",
          view: "checks",
          measure: "net_revenue",
          label: "net_revenue",
          measure_type: "custom",
          is_composite: true,
          drillable
        }
      ],
      edges: []
    }
  });
  useTimeDimensions.mockReturnValue({
    data: { by_view: { checks: ["checks.created_at"] } },
    isPending: false
  });
  useOpportunityQuery.mockReturnValue({ data: opp, isPending: false, error: null });
  useDrillQuery.mockReturnValue({ data: undefined, isPending: false, error: null });

  return render(
    <WorldModelInstanceOpportunity
      measureName='net_revenue'
      induced={false}
      promotedFrom={undefined}
      entityView='checks'
      additivity={additivity}
      instance={INSTANCE}
      model={EMPTY_MODEL}
      onSelect={vi.fn()}
    />
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("WorldModelInstanceOpportunity", () => {
  it("keeps the decomposition affordance for a drillable passthrough composite", () => {
    // `checks.net_revenue` is an engine-accepted additive composite. airlayer
    // classifies it Passthrough (Number|Custom -> Passthrough), NOT Additive —
    // so a bare `sizable` (additive | non_additive) gate returns null before the
    // section ever mounts, killing the affordance on exactly the measures this
    // whole feature exists to expose. Post-merge there is one section, so the
    // gate must be `sizable || drillEnabled` and the section's own query gate
    // must admit a drillable node too.
    //
    // Asserting the collapsed chevron alone is not enough: that passes even if
    // the section opens onto "No addressable upside to size" with no drill
    // affordance anywhere. Expand it and check the real per-row decomposition.
    renderInstanceOpportunity("passthrough", true);

    expect(screen.getByTestId("wm-opp-toggle-checks.net_revenue")).toBeDefined();
    fireEvent.click(screen.getByTestId("wm-opp-toggle-checks.net_revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-checks.channel"));
    expect(screen.getByTestId("wm-opp-drill-toggle-mobile_app")).toBeDefined();
  });

  it("renders nothing for a passthrough the engine does not accept (drillable: false)", () => {
    // A passthrough the engine refuses (nested/cross-view/multiplicative refs)
    // is neither sizable nor drillable, so the whole instance row stays empty.
    const { container } = renderInstanceOpportunity("passthrough", false);
    expect(container.firstChild).toBeNull();
  });

  it("renders one merged section, not a separate drill section", () => {
    renderInstanceOpportunity("additive", true);
    expect(screen.queryByTestId("wm-drill-toggle-checks.net_revenue")).toBeNull();
    expect(screen.getByTestId("wm-opp-toggle-checks.net_revenue")).toBeTruthy();
  });

  it("does not fetch any chain until a row is expanded", () => {
    renderInstanceOpportunity("additive", true);
    // Collapsed: the eager (hideWhenEmpty) opportunity scan runs; no drill does.
    expect(useDrillQuery).not.toHaveBeenCalled();

    // Mounted rows are the real question — a collapsed section trivially fetches
    // nothing. Open the section and its top dimension so the rows exist, and
    // assert they still cost zero drill queries while their own toggles are shut.
    fireEvent.click(screen.getByTestId("wm-opp-toggle-checks.net_revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-checks.channel"));
    expect(screen.getByTestId("wm-opp-drill-toggle-mobile_app")).toBeDefined();
    expect(useDrillQuery).not.toHaveBeenCalled();

    // …and only then does expanding a row issue one.
    fireEvent.click(screen.getByTestId("wm-opp-drill-toggle-mobile_app"));
    expect(useDrillQuery).toHaveBeenCalled();
  });
});
