// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MetricEdge, MetricNode } from "@/types/metricTree";
import { ImpactList } from "./ImpactList";
import type { ScenarioNodeData } from "./nodeValue";

afterEach(cleanup);

function node(id: string, label: string): MetricNode {
  return { id, label } as MetricNode;
}

function edge(from: string, to: string, over: Partial<MetricEdge> = {}): MetricEdge {
  return {
    from,
    to,
    kind: "driver",
    direction: "positive",
    strength: "moderate",
    confidence: "medium",
    form: "linear",
    ...over
  };
}

function dataMap(entries: ScenarioNodeData[]): Map<string, ScenarioNodeData> {
  return new Map(entries.map((d) => [d.node.id, d]));
}

describe("ImpactList", () => {
  it("lists an impacted measure so it is findable without hunting the canvas", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          { node: node("menu_items.avg_unit_price", "Average menu price"), state: "lever" },
          {
            node: node("menu_items.avg_unit_margin", "Average retained margin"),
            state: "impacted",
            delta: -3,
            confidence: "exact"
          },
          { node: node("orders.far", "Untouched"), state: "unreachable" }
        ])}
        onSelect={vi.fn()}
      />
    );
    const list = screen.getByTestId("scenario-impact-list");
    expect(list).toHaveTextContent("Average retained margin");
    expect(list).toHaveTextContent("-3");
    // The lever itself is not an "impact", and untouched measures are not listed.
    expect(list).not.toHaveTextContent("Untouched");
  });

  it("says so explicitly when a lever moves nothing, rather than rendering empty", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          { node: node("a", "Solo lever"), state: "lever" },
          { node: node("b", "Untouched"), state: "unreachable" }
        ])}
        onSelect={vi.fn()}
      />
    );
    expect(screen.getByTestId("scenario-impact-list")).toHaveTextContent(
      /moves no other modelled measure/i
    );
  });

  // The failure this whole panel was built to avoid, in its last hiding place:
  // when no lever resolves, `predict` is never called, and an empty impact set
  // is the absence of an answer rather than an answer of "nothing".
  it("distinguishes a simulation that never ran from a lever that moves nothing", () => {
    render(
      <ImpactList
        nodeData={dataMap([{ node: node("a", "Solo lever"), state: "lever" }])}
        onSelect={vi.fn()}
        runState='unresolved'
      />
    );
    const list = screen.getByTestId("scenario-impact-list");
    expect(list).toHaveTextContent(/nothing was simulated/i);
    expect(list).not.toHaveTextContent(/moves no other modelled measure/i);
  });

  it("labels each impact exact or estimated so the two never read alike", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          {
            node: node("orders.prime_cost", "Prime cost"),
            state: "impacted",
            baseline: 100,
            simulated: 150,
            delta: 50,
            confidence: "exact"
          },
          {
            node: node("orders.net_sales", "Net sales"),
            state: "impacted",
            baseline: 200,
            simulated: 260,
            delta: 60,
            confidence: "estimated"
          }
        ])}
        onSelect={vi.fn()}
      />
    );
    expect(screen.getByTestId("scenario-confidence-exact")).toBeInTheDocument();
    expect(screen.getByTestId("scenario-confidence-estimated")).toBeInTheDocument();
    // Spelled out in the panel, where there is room — the canvas has only the
    // glyph, and this is where it gets learned.
    expect(screen.getByTestId("scenario-impact-list")).toHaveTextContent("estimated");
  });

  it("keeps an unquantifiable impact listed but unsized", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          {
            node: node("orders.revenue", "Revenue"),
            state: "unquantifiable",
            confidence: "unquantifiable"
          }
        ])}
        onSelect={vi.fn()}
      />
    );
    const list = screen.getByTestId("scenario-impact-list");
    expect(list).toHaveTextContent("Revenue");
    expect(list).toHaveTextContent(/can't size/i);
    expect(list).not.toHaveTextContent(/(^|[^.\d])0([^.\d]|$)/);
  });

  const impacted: ScenarioNodeData = {
    node: node("orders.net_sales", "net_sales"),
    state: "impacted",
    baseline: 200,
    simulated: 260,
    delta: 60,
    confidence: "estimated",
    path: ["orders.checks", "orders.net_sales"]
  };

  it("expands a row into the route behind its number", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{ edges: [edge("orders.checks", "orders.net_sales")] }}
        fitted={[{ from: "orders.checks", to: "orders.net_sales", coefficient: 0.84, n: 180 }]}
        leverIds={["orders.checks"]}
      />
    );
    expect(screen.queryByTestId("scenario-impact-detail-orders.net_sales")).toBeNull();

    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));

    const detail = screen.getByTestId("scenario-impact-detail-orders.net_sales");
    expect(detail).toHaveTextContent("checks → net_sales");
    // The number alone is not the answer to "on what basis" — where the
    // coefficient came from is the difference between an assertion and evidence.
    expect(detail).toHaveTextContent(/fitted from history/i);
    expect(detail).toHaveTextContent("n=180");
  });

  // `n` says how much evidence there was, not how far the estimate is from
  // zero nor over what range it holds. The second is a limit on the answer:
  // propagation refuses a lever outside the observed spread rather than
  // extrapolating, so the range bounds what the panel can be asked.
  it("shows the fit's t-statistic and the range it was measured over", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{ edges: [edge("orders.checks", "orders.net_sales")] }}
        fitted={[
          {
            from: "orders.checks",
            to: "orders.net_sales",
            coefficient: 0.84,
            n: 180,
            t_stat: 6.2,
            domain: [12, 340]
          }
        ]}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    const detail = screen.getByTestId("scenario-impact-detail-orders.net_sales");
    expect(detail).toHaveTextContent(/\(n=180, t=6\.20\)/);
    expect(detail).toHaveTextContent(/measured over 12\.00–340\.00/);
  });

  // `skip_serializing_if` on a git-pinned struct is a serde attribute, not a
  // guarantee, so an absent `n` can arrive as either encoding. A strict
  // `=== undefined` read let the null through to `null.toLocaleString()` — a
  // TypeError in the render phase, which on a panel with no error boundary
  // takes the page with it.
  it("survives an n that arrives as null rather than absent", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{ edges: [edge("orders.checks", "orders.net_sales")] }}
        fitted={[
          {
            from: "orders.checks",
            to: "orders.net_sales",
            coefficient: 0.84,
            n: null,
            t_stat: 6.2
          }
        ]}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    const detail = screen.getByTestId("scenario-impact-detail-orders.net_sales");
    expect(detail).toHaveTextContent(/\(t=6\.20\)/);
    expect(detail).not.toHaveTextContent(/n=/);
  });

  // `n` and `t_stat` are independently optional on the wire, and the stats
  // parenthetical used to be assembled around a "(" that only `n` could open —
  // so a fit carrying a t and no n dropped the t without saying so.
  it("shows the t-statistic even when the fit carries no n", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{ edges: [edge("orders.checks", "orders.net_sales")] }}
        fitted={[{ from: "orders.checks", to: "orders.net_sales", coefficient: 0.84, t_stat: 6.2 }]}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    expect(screen.getByTestId("scenario-impact-detail-orders.net_sales")).toHaveTextContent(
      /\(t=6\.20\)/
    );
  });

  it("focuses the measure on the canvas as well as expanding it", () => {
    const onSelect = vi.fn();
    render(<ImpactList nodeData={dataMap([impacted])} onSelect={onSelect} />);
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    expect(onSelect).toHaveBeenCalledWith("orders.net_sales");
  });

  it("collapses the row again on a second click", () => {
    render(<ImpactList nodeData={dataMap([impacted])} onSelect={vi.fn()} />);
    const row = screen.getByTestId("scenario-impact-row-orders.net_sales");
    fireEvent.click(row);
    expect(screen.getByTestId("scenario-impact-detail-orders.net_sales")).toBeInTheDocument();
    fireEvent.click(row);
    expect(screen.queryByTestId("scenario-impact-detail-orders.net_sales")).toBeNull();
  });

  it("keeps one row open at a time so the list stays in view", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          impacted,
          {
            node: node("orders.prime_cost", "prime_cost"),
            state: "impacted",
            delta: 10,
            confidence: "exact"
          }
        ])}
        onSelect={vi.fn()}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.prime_cost"));
    expect(screen.queryByTestId("scenario-impact-detail-orders.net_sales")).toBeNull();
    expect(screen.getByTestId("scenario-impact-detail-orders.prime_cost")).toBeInTheDocument();
  });

  // `predict` sums the delta over every route into a node but returns only the
  // first route's ids. Presenting that route as the sole cause of the total is
  // wrong by construction, so the panel has to say how many there were.
  it("says the total sums several routes when more than one reaches the measure", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{
          edges: [
            edge("orders.checks", "orders.net_sales", { coefficient: 0.5 }),
            edge("orders.checks", "orders.covers", { coefficient: 0.5 }),
            edge("orders.covers", "orders.net_sales", { coefficient: 0.5 })
          ]
        }}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    expect(screen.getByTestId("scenario-impact-multipath-orders.net_sales")).toHaveTextContent(
      /2 routes reach this measure/i
    );
  });

  // The sentence claims the figure sums these routes. A route through an edge
  // nothing sized reaches the measure but adds no magnitude, so counting it
  // would describe a sum that never happened — here that leaves one route, and
  // the multi-route paragraph must not render at all.
  it("does not count a route whose edge carries no magnitude", () => {
    render(
      <ImpactList
        nodeData={dataMap([impacted])}
        onSelect={vi.fn()}
        tree={{
          edges: [
            edge("orders.checks", "orders.net_sales", { coefficient: 0.5 }),
            edge("orders.checks", "orders.covers", { coefficient: 0.5 }),
            // Never sized — declared nor fitted.
            edge("orders.covers", "orders.net_sales")
          ]
        }}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.net_sales"));
    expect(screen.queryByTestId("scenario-impact-multipath-orders.net_sales")).toBeNull();
  });

  // `predict` hardcodes `form: linear` on an unquantifiable impact, where 0
  // means UNKNOWN. Expanding one must not turn either placeholder into a claim.
  it("expands an unquantifiable impact onto the edge that has no magnitude, with no number", () => {
    render(
      <ImpactList
        nodeData={dataMap([
          {
            node: node("orders.revenue", "revenue"),
            state: "unquantifiable",
            confidence: "unquantifiable",
            path: ["orders.checks", "orders.revenue"]
          }
        ])}
        onSelect={vi.fn()}
        tree={{ edges: [edge("orders.checks", "orders.revenue")] }}
        fitted={[
          { from: "orders.checks", to: "orders.revenue", refusal: "the driver does not vary" }
        ]}
        leverIds={["orders.checks"]}
      />
    );
    fireEvent.click(screen.getByTestId("scenario-impact-row-orders.revenue"));

    const detail = screen.getByTestId("scenario-impact-detail-orders.revenue");
    expect(detail).toHaveTextContent("checks → revenue");
    expect(detail).toHaveTextContent(/the driver does not vary/i);
    expect(detail).not.toHaveTextContent(/coefficient \d/);
  });
});
