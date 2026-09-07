// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import type { MetricNode, MetricTree } from "@/types/metricTree";
import { LeverList } from "./LeverList";
import type { ScenarioNodeData } from "./nodeValue";
import type { ScenarioState } from "./scenarioUrl";

// Each lever row carries a Radix slider, which measures its own thumb. jsdom
// ships no ResizeObserver, so without this the whole list fails to mount.
beforeAll(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
  );
});

afterEach(cleanup);

const tree = {
  nodes: [
    { id: "stores.net_sales", label: "Net sales" } as MetricNode,
    { id: "stores.store_profit", label: "Store profit" } as MetricNode
  ],
  edges: [{ from: "stores.net_sales", to: "stores.store_profit" }]
} as unknown as MetricTree;

const state: ScenarioState = {
  levers: [{ nodeId: "stores.net_sales", raw: "+5%" }],
  periodDays: 90,
  timeDimension: "stores.business_date",
  instance: null
};

/** The header figures only — the slider below carries its own `%` end labels,
 *  which would satisfy a whole-row assertion without the move ever rendering. */
function renderValue(nodeData: ScenarioNodeData): HTMLElement {
  render(
    <LeverList
      tree={tree}
      state={state}
      onChange={vi.fn()}
      conflicts={[]}
      leverErrors={new Map()}
      nodeData={new Map([[nodeData.node.id, nodeData]])}
    />
  );
  return screen.getByTestId("scenario-lever-value-stores.net_sales");
}

/** The fit findings sit behind a disclosure — one line per edge is more panel
 *  than the levers themselves get, so nothing below is in the DOM until it is
 *  opened. */
function openDriverSizing() {
  fireEvent.click(screen.getByTestId("scenario-driver-sizing-toggle"));
}

describe("LeverList", () => {
  // A lever on a view the window is not anchored on gets no baseline, and the
  // generic "no baseline value" gave no hint that the WINDOW was the cause —
  // the scenario looks healthy, the other lever resolves, and the picker's
  // `foreign` check passes because the dimension does belong to a lever's
  // view, just not this one's.
  it("blames the window when a lever's view is not the anchored one", () => {
    const twoViews = {
      nodes: [
        { id: "stores.net_sales", label: "Net sales" } as MetricNode,
        { id: "quickbooks_pl.net_income", label: "Net income" } as MetricNode
      ],
      edges: []
    } as unknown as MetricTree;

    render(
      <LeverList
        tree={twoViews}
        state={{
          ...state,
          levers: [{ nodeId: "quickbooks_pl.net_income", raw: "+5%" }]
        }}
        onChange={vi.fn()}
        conflicts={[]}
        leverErrors={new Map([["quickbooks_pl.net_income", "no_baseline"]])}
        nodeData={new Map()}
      />
    );

    const row = screen.getByTestId("scenario-lever-quickbooks_pl.net_income");
    expect(row).toHaveTextContent("stores");
    expect(row).toHaveTextContent("quickbooks_pl");
    // Not the generic line, which names no cause and no fix.
    expect(row).not.toHaveTextContent("no baseline value, so only a signed delta");
  });

  it("keeps the generic no-baseline line for a lever on the anchored view", () => {
    render(
      <LeverList
        tree={tree}
        state={state}
        onChange={vi.fn()}
        conflicts={[]}
        leverErrors={new Map([["stores.net_sales", "no_baseline"]])}
        nodeData={new Map()}
      />
    );
    expect(screen.getByTestId("scenario-lever-stores.net_sales")).toHaveTextContent(
      "no baseline value, so only a signed delta (+3) works here"
    );
  });

  // The whole point of a lever row is the move. Showing the baseline alone
  // means the row reports the value the analyst just replaced.
  it("shows the lever's new value and the size of the move", () => {
    const value = renderValue({
      node: tree.nodes[0],
      state: "lever",
      baseline: 100,
      simulated: 105,
      delta: 5,
      leverRaw: "+5%"
    });
    expect(value).toHaveTextContent("100.00");
    expect(value).toHaveTextContent("105.00");
    expect(value).toHaveTextContent("+5.00");
    expect(value).toHaveTextContent("+5.0%");
  });

  it("shows a bare delta when the lever has no baseline to move from", () => {
    const value = renderValue({
      node: tree.nodes[0],
      state: "lever",
      delta: -3,
      leverRaw: "-3"
    });
    expect(value).toHaveTextContent("-3.00");
    expect(value).toHaveTextContent("Δ");
  });

  it("shows the baseline alone while the lever still sits on it", () => {
    const value = renderValue({
      node: tree.nodes[0],
      state: "lever",
      baseline: 100,
      leverRaw: "100"
    });
    expect(value).toHaveTextContent("100.00");
    expect(value).not.toHaveTextContent("→");
    expect(value).not.toHaveTextContent("%");
  });

  // The slider writes percentages and nothing else, so without a value to
  // scale it can only ever produce a lever `resolveLever` rejects. Offering it
  // anyway is what made a refused baseline look like a broken panel.
  // Radix marks its thumb with `data-disabled` rather than the `disabled`
  // attribute jest-dom's `toBeDisabled` looks for — a `span` can't carry one.
  it("disables the % slider when the lever has no value to scale", () => {
    renderValue({ node: tree.nodes[0], state: "lever", leverRaw: "" });
    expect(screen.getByRole("slider")).toHaveAttribute("data-disabled");
    expect(screen.getByTestId("scenario-slider-unscalable")).toHaveTextContent(/signed delta/i);
  });

  it("disables the % slider when the lever's value is 0", () => {
    // A percentage of nothing is nothing — the same dead control, for a
    // reason worth naming separately from a missing baseline.
    renderValue({ node: tree.nodes[0], state: "lever", baseline: 0, leverRaw: "" });
    expect(screen.getByRole("slider")).toHaveAttribute("data-disabled");
    expect(screen.getByTestId("scenario-slider-unscalable")).toHaveTextContent(/is 0 over/i);
  });

  // A refusal that names a statistical cause but not its sample can't be
  // checked from the screen — a collapsed panel and a genuinely flat driver
  // read identically, which is what made one of these take three rounds to
  // pin down.
  it("says how much history a refused fit actually saw", () => {
    render(
      <LeverList
        tree={tree}
        state={state}
        onChange={vi.fn()}
        conflicts={[]}
        leverErrors={new Map()}
        nodeData={new Map()}
        fitted={[
          {
            from: "stores.net_sales",
            to: "stores.store_profit",
            n: 13498,
            n_panels: 360,
            refusal: "the driver does not vary within any panel"
          }
        ]}
      />
    );
    openDriverSizing();
    const note = screen.getByTestId("scenario-fit-refused");
    expect(note).toHaveTextContent("13,498 paired observations");
    expect(note).toHaveTextContent("360 panels");
  });

  it("leaves the slider live once the lever has a value to scale", () => {
    renderValue({ node: tree.nodes[0], state: "lever", baseline: 100, leverRaw: "+5%" });
    expect(screen.getByRole("slider")).not.toHaveAttribute("data-disabled");
    expect(screen.queryByTestId("scenario-slider-unscalable")).not.toBeInTheDocument();
  });
});

/** The whole list, so the `fits` / `refusals` / `noDriverLevers` blocks — the
 *  three that carry every fit finding and are otherwise unrendered by any
 *  test — are actually mounted. */
function renderList(props: Partial<Parameters<typeof LeverList>[0]> = {}) {
  render(
    <LeverList
      tree={tree}
      state={state}
      onChange={vi.fn()}
      conflicts={[]}
      leverErrors={new Map()}
      nodeData={new Map()}
      {...props}
    />
  );
}

describe("LeverList — fits and refusals", () => {
  // A sentence per edge is more panel than the levers get. Collapsed, the one
  // thing that still has to reach the screen is that some edges were NOT
  // sized — that is why a branch of the canvas shows nothing, and a reader who
  // never opens this must still see it.
  it("keeps the per-edge findings collapsed but counts both halves", () => {
    renderList({
      fitted: [
        { from: "stores.net_sales", to: "stores.store_profit", coefficient: 0.31, profile: [] },
        { from: "stores.net_sales", to: "stores.gross_margin", coefficient: null, refusal: "flat" }
      ]
    });
    const toggle = screen.getByTestId("scenario-driver-sizing-toggle");
    expect(toggle).toHaveTextContent("1 sized from history");
    expect(toggle).toHaveTextContent("1 not sized");
    expect(screen.queryByTestId("scenario-fitted-coefficients")).toBeNull();
    expect(screen.queryByTestId("scenario-fit-refused")).toBeNull();

    openDriverSizing();
    expect(screen.getByTestId("scenario-fitted-coefficients")).toBeTruthy();
    expect(screen.getByTestId("scenario-fit-refused")).toBeTruthy();
  });

  it("renders nothing at all when there is no fit to report", () => {
    renderList({ fitted: [] });
    expect(screen.queryByTestId("scenario-driver-sizing-toggle")).toBeNull();
  });

  // `coefficient` is `Option<f64>` behind `skip_serializing_if`, a serde
  // attribute on a git-pinned struct. Under `!== undefined` a pin that stopped
  // skipping nulls would file every refusal as a fit and every fit as a
  // refusal — each edge rendering as the opposite of what it is.
  it("reads an explicit null coefficient as a refusal, not a fit", () => {
    renderList({
      fitted: [
        {
          from: "stores.net_sales",
          to: "stores.store_profit",
          coefficient: null,
          refusal: "the driver does not vary within any panel"
        }
      ]
    });
    openDriverSizing();
    expect(screen.queryByTestId("scenario-fitted-coefficients")).toBeNull();
    expect(screen.getByTestId("scenario-fit-refused")).toBeTruthy();
  });

  it("renders a real coefficient as a fit", () => {
    renderList({
      fitted: [
        {
          from: "stores.net_sales",
          to: "stores.store_profit",
          coefficient: 0.31,
          n: 1,
          n_panels: 1,
          profile: []
        }
      ]
    });
    openDriverSizing();
    expect(screen.getByTestId("scenario-fitted-coefficients")).toBeTruthy();
    expect(screen.queryByTestId("scenario-fit-refused")).toBeNull();
  });

  it("says panel, not panels, for a single panel", () => {
    renderList({
      fitted: [
        {
          from: "stores.net_sales",
          to: "stores.store_profit",
          coefficient: 0.31,
          n: 1,
          n_panels: 1,
          profile: []
        }
      ]
    });
    openDriverSizing();
    const block = screen.getByTestId("scenario-fitted-coefficients");
    // `n` and `n_panels` only render when both are non-zero AND the fit
    // reports them; assert on whichever half is present rather than the
    // whole sentence, so this pins the plural rule and not the layout.
    expect(block.textContent).not.toContain("panels");
  });

  // The finding this closes: `fitted` only ever carries UNdeclared edges, so a
  // lever whose other edges are declared drivers or component expressions can
  // never be cleared by looking at `fitted` alone. It was being told it "moves
  // nothing" while propagating perfectly well.
  it("does not tell a lever with a declared edge that it moves nothing", () => {
    const declaredTree = {
      nodes: tree.nodes,
      edges: [
        { from: "stores.net_sales", to: "stores.store_profit", kind: "driver", coefficient: 0.4 },
        { from: "stores.net_sales", to: "stores.gross_margin", kind: "driver" }
      ]
    } as unknown as MetricTree;
    renderList({
      tree: declaredTree,
      fitted: [
        {
          from: "stores.net_sales",
          to: "stores.gross_margin",
          coefficient: null,
          refusal: "not enough history"
        }
      ]
    });
    expect(screen.queryByText(/moves nothing this scenario can size/)).toBeNull();
  });

  it("still says so when every outgoing edge was refused", () => {
    const undeclaredTree = {
      nodes: tree.nodes,
      edges: [{ from: "stores.net_sales", to: "stores.store_profit", kind: "driver" }]
    } as unknown as MetricTree;
    renderList({
      tree: undeclaredTree,
      fitted: [
        {
          from: "stores.net_sales",
          to: "stores.store_profit",
          coefficient: null,
          refusal: "not enough history"
        }
      ]
    });
    expect(screen.getByText(/moves nothing this scenario can size/)).toBeTruthy();
  });
});

describe("LeverList — the baseline note", () => {
  // The server composes one note from the engine outcome AND the views the
  // read skipped, so the two arrive together. Treating the note's presence as
  // "there is no baseline" printed "only signed-delta levers can be sized"
  // above a lever whose % had already resolved against a real 98.60k.
  it("does not claim there is no baseline when measures were valued", () => {
    renderList({
      baselineNote:
        "`daily_operations`, `quickbooks_pl` were not read: no `sales_daily.business_date` to anchor the window on",
      anyValued: true
    });
    const note = screen.getByTestId("scenario-baseline-failed");
    expect(note).toHaveTextContent("Part of the baseline is missing");
    expect(note.textContent).not.toContain("No baseline to anchor to");
    // The reason still has to reach the screen — it names the fix.
    expect(note).toHaveTextContent("quickbooks_pl");
  });

  it("says the stronger sentence when nothing was valued at all", () => {
    renderList({
      baselineNote: "no rows in this period on `stores.business_date`",
      anyValued: false
    });
    expect(screen.getByTestId("scenario-baseline-failed")).toHaveTextContent(
      "No baseline to anchor to"
    );
  });
});
