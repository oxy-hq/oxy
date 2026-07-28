// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorldModel } from "@/types/worldModel";
import { presetPeriod, WorldModelOpportunitiesSection } from "./WorldModelOpportunitiesSection";

// The section reaches the API only through these hooks, so mocking them covers
// every request the component (and its per-row drill) can make — no
// QueryClientProvider needed.
const useTimeDimensions = vi.hoisted(() => vi.fn());
const useOpportunityQuery = vi.hoisted(() => vi.fn());
const useMetricTree = vi.hoisted(() => vi.fn());
const useDrillQuery = vi.hoisted(() => vi.fn());

vi.mock("@/hooks/api/useMetricTree", () => ({
  useTimeDimensions,
  useOpportunityQuery,
  useMetricTree,
  useDrillQuery
}));

const EMPTY_MODEL: WorldModel = { entities: [], edges: [] };

function renderSection(
  additivity: string,
  instance?: { entity: string; key: string },
  /** `null` models a node the tree doesn't carry — the fail-closed case. */
  drillable: boolean | null = false,
  edges: unknown[] = []
) {
  // The section reads the tree for the node's own `drillable`.
  useMetricTree.mockReturnValue({
    data: {
      nodes:
        drillable === null
          ? []
          : [
              {
                id: "order.revenue",
                view: "order",
                measure: "revenue",
                label: "revenue",
                measure_type: "sum",
                is_composite: false,
                drillable
              }
            ],
      edges
    }
  });
  useDrillQuery.mockReturnValue({ data: undefined, isPending: false, error: null });
  return render(
    <WorldModelOpportunitiesSection
      nodeId='order.revenue'
      view='order'
      additivity={additivity}
      periodDays={90}
      onPeriodDaysChange={vi.fn()}
      model={EMPTY_MODEL}
      onSelect={vi.fn()}
      instance={instance}
    />
  );
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("presetPeriod", () => {
  it("ends yesterday, so a partial current day never reads as a depressed segment", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-14T09:30:00Z"));

    expect(presetPeriod(90)).toEqual(["2026-04-15", "2026-07-13"]);
  });

  it("crosses a year boundary without drifting", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-05T00:00:00Z"));

    expect(presetPeriod(30)).toEqual(["2025-12-06", "2026-01-04"]);
  });
});

describe("WorldModelOpportunitiesSection", () => {
  it("sizes an additive sum on a per-unit rate, surfacing the addressable upside", () => {
    // On a SUM the engine compares per-unit RATES (total ÷ rows) and sizes the
    // gap by the segment's own volume, so a small segment can't masquerade as
    // headroom. The upside IS the actionable number here — it must be shown.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.revenue",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 100000,
        weight_basis: "rows",
        skipped_dimensions: [],
        downstream: [],
        dimensions: [
          {
            dimension: "store_region",
            cardinality: 4,
            benchmark_basis: "p75",
            total_upside: 12000,
            other_segments_skipped: 2,
            segments_dropped_as_noise: 0,
            segments: [
              {
                segment: "South",
                current_value: 21,
                volume: 400,
                benchmark: 30,
                gap: 9,
                upside: 3600
              }
            ]
          }
        ]
      },
      isPending: false,
      error: null
    });

    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    // Headline "biggest lever" + the sized upside are surfaced (not suppressed).
    expect(screen.getByText(/Biggest lever/)).toBeDefined();
    expect(screen.getAllByText("+12.0k").length).toBeGreaterThan(0);
    // ...and its share of the measure total (12000 / 100000), so the absolute
    // upside has a relative scale.
    expect(screen.getAllByText("+12%").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByTestId("wm-opp-dim-store_region"));
    expect(screen.getByText("South")).toBeDefined();
    // The rate gap and the volume it applies to, both named in the view's own
    // units — the reader can multiply 9 × 400 back to the +3.6k beside them.
    expect(screen.getByText("21.0 → 30.0 per order")).toBeDefined();
    expect(screen.getByText("400 orders")).toBeDefined();
    expect(screen.getByText("+3.6k")).toBeDefined();
    // Segment upside as a share of total (3600 / 100000), one decimal below 10%.
    expect(screen.getByText("+3.6%")).toBeDefined();
  });

  it("names the rate's unit rather than leaving it a bare number", () => {
    // The reported bug: "rate 533.9 vs 801.6" gave no way to learn the figures
    // were revenue PER ORDER, so the panel's own arithmetic was unreadable.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.revenue",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 100000,
        weight_basis: "rows",
        rate_denominator: "order.total_orders",
        skipped_dimensions: [],
        downstream: [],
        dimensions: [channelOnly(0)]
      },
      isPending: false,
      error: null
    });

    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-channel"));

    // The unit is stated where the numbers are, not only in a tooltip.
    expect(screen.getByText("· rate per order · % of 90d total")).toBeDefined();
    expect(screen.getByText("21.0 → 30.0 per order")).toBeDefined();
  });

  /** One dimension, one proven segment, two dropped for want of evidence. */
  const channelOnly = (dropped: number) => ({
    dimension: "channel",
    cardinality: 4,
    benchmark_basis: "best_peer",
    total_upside: 12000,
    other_segments_skipped: 0,
    segments_dropped_as_noise: dropped,
    segments: [
      { segment: "mobile_app", current_value: 21, volume: 400, benchmark: 30, gap: 9, upside: 3600 }
    ]
  });

  const mockOpp = (dimensions: unknown[]) => {
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.revenue",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 100000,
        weight_basis: "rows",
        skipped_dimensions: [],
        downstream: [],
        dimensions
      },
      isPending: false,
      error: null
    });
  };

  it("names the period its percentages are shares of", () => {
    // Two claims sit inches apart: the measure's headline value (all time) and
    // these percentages (the selected period). A reader who checks the obvious
    // way — upside ÷ the big number above — gets a different answer and
    // concludes the panel is broken. So the denominator is named.
    mockOpp([channelOnly(0)]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.getByText(/of 90d/)).toBeDefined();
  });

  it("declares the segments it dropped, and does not claim to have sized them", () => {
    // The total covers only what cleared the gate. Saying "each below-benchmark
    // segment" directly above a line admitting two were dropped claims a scope
    // the number does not have.
    mockOpp([channelOnly(2)]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    expect(screen.getByText(/each segment with a provable shortfall/)).toBeDefined();
    expect(screen.queryByText(/each below-benchmark segment/)).toBeNull();

    fireEvent.click(screen.getByTestId("wm-opp-dim-channel"));
    expect(screen.getByText(/2 below benchmark but within sampling noise/)).toBeDefined();
  });

  it("promises a peer's rate only when the benchmark is actually a peer's", () => {
    // A p75 benchmark is an interpolated percentile no segment need actually
    // have, so "reached its peer rate" would name a target that doesn't exist.
    mockOpp([channelOnly(0)]);
    const bestPeer = renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.getByText(/reached its best peer's rate/)).toBeDefined();
    bestPeer.unmount();

    mockOpp([{ ...channelOnly(0), benchmark_basis: "p75" }]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.getByText(/reached its peers' p75 rate/)).toBeDefined();
    expect(screen.queryByText(/best peer/)).toBeNull();
  });

  it("states the scan's real boundary instead of claiming the whole warehouse", () => {
    // The scan walks this view's dimensions plus one hop through each foreign
    // entity — not the warehouse. A reader who believes it was exhaustive reads
    // a missing lever as evidence that no lever exists.
    mockOpp([channelOnly(0)]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    // Matched on the whole rendered sentence: the view name sits in its own
    // <span>, which the default text matcher would skip.
    expect(
      screen.getByText(
        (_, el) =>
          el?.tagName === "P" &&
          /scans dimensions on order and one join hop/.test(
            (el.textContent ?? "").replace(/\s+/g, " ")
          )
      )
    ).toBeDefined();
    expect(screen.queryByText(/every segmentable dimension in the warehouse/)).toBeNull();
  });

  it("does not pass off capped-out segments as merely minor ones", () => {
    // `other_segments_skipped` mixes sub-1% tail with anything past the top-5
    // cap, and the latter need not be small — while the dimension total above
    // still counts them.
    mockOpp([{ ...channelOnly(0), other_segments_skipped: 2 }]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-channel"));

    expect(screen.getByText(/2 more segments not shown/)).toBeDefined();
    expect(screen.queryByText(/lower-upside/)).toBeNull();
  });

  it("warns levers overlap only when there is more than one to add up", () => {
    // The dimensions are alternative cuts of the same rows and a ranked stack
    // invites addition — but a caution that fires where it cannot apply is how
    // readers learn to skip the cautions.
    mockOpp([channelOnly(0)]);
    const single = renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.queryByText(/Levers overlap/)).toBeNull();
    single.unmount();

    mockOpp([channelOnly(0), { ...channelOnly(0), dimension: "region", total_upside: 8000 }]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.getByText(/Levers overlap/)).toBeDefined();
  });

  it("refuses to size a sum whose view declares no count measure", () => {
    // Without a `count` measure there is no volume to normalize totals into
    // comparable rates, so the engine refuses rather than compare raw totals.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.revenue",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 100000,
        weight_basis: "rows",
        dimensions: [],
        skipped_dimensions: [
          {
            dimension: "order.store_region",
            reason:
              "'order.revenue' is an additive total; sizing it fairly needs a per-row `count` measure on view 'order' to compare per-unit rates, but none is declared"
          }
        ],
        downstream: []
      },
      isPending: false,
      error: null
    });

    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    expect(screen.getByText(/Add a/)).toBeDefined();
    expect(screen.getByText("type: count")).toBeDefined();
  });

  it("renders no section at all for a measure that can never be sized", () => {
    // A non-additive measure (an average) only ever yields a rate spread — a gap
    // in the measure's own units with no amount attached. It used to get a
    // "Segment spread · rates only · diagnostic" section; the panel now shows
    // opportunities or nothing, and this is knowable from the type alone.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({ data: undefined, isPending: false, error: null });

    const { container } = renderSection("non_additive");

    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText(/Segment spread/)).toBeNull();
    // ...and it must not cost a warehouse scan to discover that.
    expect(useOpportunityQuery).toHaveBeenCalledWith(null, false);
  });

  it("never ranks a mode that carries no upside under an upside heading", () => {
    // A count/min/max is additive, so it is worth asking about — but the engine
    // answers with value_share, whose `upside` is a raw gap, not an amount. The
    // old spread rows showed it anyway, under "Opportunities · addressable
    // upside". Now the dimensions are dropped and the reason is stated.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.total_orders",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 4000,
        weight_basis: "value_share",
        skipped_dimensions: [],
        downstream: [],
        dimensions: [{ ...channelOnly(0), total_upside: 999 }]
      },
      isPending: false,
      error: null
    });

    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    expect(screen.getByText(/No addressable upside to size/)).toBeDefined();
    // The engine's meaningless upside must not reach the DOM in any form.
    expect(screen.queryByText(/999/)).toBeNull();
    expect(screen.queryByTestId("wm-opp-dim-channel")).toBeNull();
    // ...and it must not be mistaken for flat data, which is a different finding.
    expect(screen.queryByText(/enough spread/)).toBeNull();
  });

  // ── The decomposition gate (`drillEnabled`), inherited from the deleted
  // standalone drill section. These four cases were that section's gating
  // tests; the predicate moved here when the two sections merged, so the
  // coverage moved with it.

  /** What the engine really returns for a `type: count`: `value_share`, never
   *  `rows`. A count measure cannot produce the rows-mode fixture `mockOpp`
   *  builds, so any count-measure assertion made against that fixture is
   *  vacuous — it tests a response the engine cannot send. */
  const mockValueShareOpp = () => {
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.total_orders",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 4000,
        weight_basis: "value_share",
        skipped_dimensions: [],
        downstream: [],
        dimensions: [channelOnly(0)]
      },
      isPending: false,
      error: null
    });
  };

  it("offers a measure-level decomposition for a type: count measure, which isn't rate-sized", () => {
    // `drillable` (supports_rate_basis) is deliberately `false` for count/min/max
    // — the engine won't form a per-unit RATE for them — but `opportunity()`
    // still decomposes them through the value-share path. The rows-mode gate
    // correctly drops the ranked rows for `value_share`, which also removes the
    // only place a per-row drill could hang off; without a measure-level chain
    // the affordance vanishes entirely — the same fail-closed capability loss
    // §5c of the explainer records.
    mockValueShareOpp();
    renderSection("additive", undefined, false);
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    // No ranked rows (the upside gate holds) …
    expect(screen.getByText(/No addressable upside to size/)).toBeDefined();
    expect(screen.queryByTestId("wm-opp-dim-channel")).toBeNull();
    // … but the decomposition is still reachable, and costs nothing unexpanded.
    const toggle = screen.getByTestId("wm-opp-measure-drill-toggle-order.revenue");
    expect(useDrillQuery).not.toHaveBeenCalled();

    fireEvent.click(toggle);

    // Expanding genuinely issues the drill — with NO `root`, so the engine picks
    // its own top row (exactly what the deleted standalone section did).
    expect(useDrillQuery).toHaveBeenCalled();
    const [req] = useDrillQuery.mock.calls[0];
    expect(req.target).toBe("order.revenue");
    expect(req.root).toBeUndefined();
  });

  it("does not offer a measure-level decomposition when the engine refuses the measure", () => {
    // A non-drillable, non-additive passthrough must not be handed a chain the
    // engine would refuse — that was the over-admitting edge-presence gate.
    mockValueShareOpp();
    const { container } = renderSection("passthrough", undefined, false);
    expect(container).toBeEmptyDOMElement();
  });

  it("keeps the measure-level drill when a value-share skip reason merely mentions 'count'", () => {
    // `dimensions` is forced to `[]` outside rows mode, so the old
    // `dimensions.length === 0` half of `refusedNoCount` was vacuous there —
    // the predicate collapsed to a bare substring match on "count" against
    // ANY skip reason. The engine's actual skip message is `breakdown query
    // failed: {e}`, an arbitrary warehouse error, so a `type: count` measure
    // whose error happens to name a column like `order_count` would flip
    // `refusedNoCount` true: wrong "declare a count measure" copy, AND the
    // measure-level chain suppressed — even though this IS a genuine
    // type: count measure the engine can still decompose via value_share.
    useTimeDimensions.mockReturnValue({
      data: { by_view: { order: ["order.created_at"] } },
      isPending: false
    });
    useOpportunityQuery.mockReturnValue({
      data: {
        target: "order.total_orders",
        period: ["2026-04-15", "2026-07-13"],
        overall_value: 4000,
        weight_basis: "value_share",
        skipped_dimensions: [
          {
            dimension: "order.store_region",
            reason: 'breakdown query failed: column "order_count" does not exist'
          }
        ],
        downstream: [],
        dimensions: []
      },
      isPending: false,
      error: null
    });

    renderSection("additive", undefined, false);
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));

    // Must NOT render the no-count refusal — this measure was never refused.
    expect(screen.queryByText(/Add a/)).toBeNull();
    expect(screen.queryByText("type: count")).toBeNull();
    // The measure-level drill affordance must still be offered.
    expect(screen.getByTestId("wm-opp-measure-drill-toggle-order.revenue")).toBeDefined();
  });

  it("offers the section for a drillable passthrough composite, which is not additive", () => {
    // An engine-accepted composite (checks.net_revenue) classifies Passthrough,
    // never Additive. The old drill section reached it because it mounted
    // unconditionally; with one merged section the query gate itself has to
    // admit `drillable`, or the composite loses the affordance outright.
    //
    // Asserting only the collapsed chevron would pass even if the section opened
    // onto "no addressable upside" with no drill affordance at all, so expand it
    // and check the per-row decomposition really is there.
    mockOpp([channelOnly(0)]);
    renderSection("passthrough", undefined, true);

    expect(screen.getByTestId("wm-opp-toggle-order.revenue")).toBeDefined();
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-channel"));
    expect(screen.getByTestId("wm-opp-drill-toggle-mobile_app")).toBeDefined();
  });

  it("tells the reader to expand a row only when there is a row to expand", () => {
    // The hint used to be gated on `drillEnabled` alone, so a measure landing on
    // "No addressable upside to size" pointed at a row that does not exist.
    mockValueShareOpp();
    const noRows = renderSection("additive", undefined, false);
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.queryByText(/expand a row/)).toBeNull();
    expect(screen.getByText(/follow this measure's gap down/)).toBeDefined();
    noRows.unmount();

    mockOpp([channelOnly(0)]);
    renderSection("additive", undefined, false);
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    expect(screen.getByText(/expand a row/)).toBeDefined();
  });

  it("refuses a non-drillable passthrough EVEN WHEN it has component edges", () => {
    // Component edges exist for every passthrough with `{{}}` refs, including
    // the nested/cross-view/multiplicative ones the engine refuses — so edge
    // presence is not the acceptance predicate. Gating on edges once rendered a
    // fully populated chain of plausible, silently wrong numbers.
    mockOpp([channelOnly(0)]);
    const { container } = renderSection("passthrough", undefined, false, [
      { from: "order.revenue_gap", to: "order.revenue", kind: "component" }
    ]);

    expect(container).toBeEmptyDOMElement();
    expect(useOpportunityQuery).toHaveBeenCalledWith(null, false);
  });

  it("fails closed when the node isn't in the tree at all", () => {
    // No matching node (the tree hasn't resolved one yet) must fail closed, not
    // open — `drillable` defaults to `false` via `?? false`.
    mockOpp([channelOnly(0)]);
    const { container } = renderSection("passthrough", undefined, null);

    expect(container).toBeEmptyDOMElement();
  });

  it("warns that a best-peer benchmark overfits noise", () => {
    // The caveat lives on a hover badge rather than inline prose; the badge
    // carries the basis and its tooltip the warning.
    mockOpp([channelOnly(0)]);
    renderSection("additive");
    fireEvent.click(screen.getByTestId("wm-opp-toggle-order.revenue"));
    fireEvent.click(screen.getByTestId("wm-opp-dim-channel"));

    expect(screen.getByText("best peer")).toBeDefined();
  });
});
