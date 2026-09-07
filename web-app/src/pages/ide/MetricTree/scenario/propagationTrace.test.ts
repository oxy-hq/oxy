import { describe, expect, it } from "vitest";
import type { FittedDriver, MetricEdge } from "@/types/metricTree";
import { buildTrace, countPathsTo, unsizableHop } from "./propagationTrace";

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

describe("buildTrace", () => {
  it("names one hop per edge on the route", () => {
    const hops = buildTrace(
      ["a.x", "b.y", "c.z"],
      { edges: [edge("a.x", "b.y"), edge("b.y", "c.z")] },
      []
    );
    expect(hops.map((h) => [h.from, h.to])).toEqual([
      ["a.x", "b.y"],
      ["b.y", "c.z"]
    ]);
  });

  it("returns nothing for a route with no hops in it", () => {
    expect(buildTrace(undefined, { edges: [] }, [])).toEqual([]);
    expect(buildTrace(["a.x"], { edges: [] }, [])).toEqual([]);
  });

  // A declared coefficient is an assertion in a file; a fitted one is a
  // regression over the baseline window that moves when the window does. The
  // same number means different things, so the source travels with it.
  it("separates a declared coefficient from one measured off history", () => {
    const fitted: FittedDriver[] = [{ from: "b.y", to: "c.z", coefficient: 0.84, n: 180 }];
    const hops = buildTrace(
      ["a.x", "b.y", "c.z"],
      { edges: [edge("a.x", "b.y", { coefficient: 2 }), edge("b.y", "c.z")] },
      fitted
    );
    expect(hops[0]).toMatchObject({ coefficient: 2, coefficientSource: "declared" });
    expect(hops[1]).toMatchObject({ coefficient: 0.84, coefficientSource: "fitted" });
    expect(hops[1].fit?.n).toBe(180);
  });

  // A refusal is a result, not an absence — it is the reason the impact above
  // shows nothing, and it must never be mistaken for an unattempted fit.
  it("keeps a refused fit distinct from an edge nothing tried to size", () => {
    const hops = buildTrace(
      ["a.x", "b.y", "c.z"],
      { edges: [edge("a.x", "b.y"), edge("b.y", "c.z")] },
      [{ from: "a.x", to: "b.y", refusal: "the driver does not vary" }]
    );
    expect(hops[0]).toMatchObject({ coefficientSource: "refused" });
    expect(hops[0].coefficient).toBeUndefined();
    expect(hops[0].fit?.refusal).toBe("the driver does not vary");
    expect(hops[1]).toMatchObject({ coefficientSource: "none" });
  });

  it("reads a component edge's sign and claims no coefficient for it", () => {
    const hops = buildTrace(
      ["a.x", "b.y"],
      { edges: [edge("a.x", "b.y", { kind: "component", sign: -1 })] },
      []
    );
    expect(hops[0]).toMatchObject({ kind: "component", sign: -1, coefficientSource: "none" });
    // The shape belongs to a fitted driver relationship; a component edge is
    // arithmetic, and badging it `linear` would dress an identity as a model.
    expect(hops[0].form).toBeUndefined();
  });

  // A predict result can outlive the tree it was computed against (branch
  // switch, `.view.yml` edit). The route is still worth naming.
  it("still names a hop whose edge is no longer in the tree", () => {
    const hops = buildTrace(["a.x", "gone.y"], { edges: [] }, []);
    expect(hops).toHaveLength(1);
    expect(hops[0]).toMatchObject({ from: "a.x", to: "gone.y", coefficientSource: "none" });
    expect(hops[0].edge).toBeUndefined();
  });
});

describe("unsizableHop", () => {
  it("points at the first hop with no magnitude", () => {
    const hops = buildTrace(
      ["a.x", "b.y", "c.z"],
      { edges: [edge("a.x", "b.y", { coefficient: 2 }), edge("b.y", "c.z")] },
      [{ from: "b.y", to: "c.z", refusal: "not enough observations" }]
    );
    expect(unsizableHop(hops)).toMatchObject({ from: "b.y", to: "c.z" });
  });

  it("finds none when every hop is sized, so the caller can't point at a guess", () => {
    const hops = buildTrace(
      ["a.x", "b.y"],
      { edges: [edge("a.x", "b.y", { coefficient: 2 })] },
      []
    );
    expect(unsizableHop(hops)).toBeUndefined();
  });
});

describe("countPathsTo", () => {
  /** A driver edge the model could size. The bare `edge()` helper declares no
   *  coefficient, which is exactly the edge the count must skip, so every case
   *  about routes that *do* contribute has to say so explicitly. */
  function sized(from: string, to: string, over: Partial<MetricEdge> = {}): MetricEdge {
    return edge(from, to, { coefficient: 0.5, ...over });
  }

  it("counts one route when only one exists", () => {
    expect(
      countPathsTo("c.z", ["a.x"], { edges: [sized("a.x", "b.y"), sized("b.y", "c.z")] })
    ).toEqual({ count: 1, capped: false });
  });

  // `predict` sums the delta over every route into a node but returns only the
  // first one's ids. Without this count the panel would present one leg of a
  // two-leg total as the whole explanation.
  it("counts both legs of a diamond", () => {
    const edges = [
      sized("a.x", "left.y"),
      sized("a.x", "right.y"),
      sized("left.y", "c.z"),
      sized("right.y", "c.z")
    ];
    expect(countPathsTo("c.z", ["a.x"], { edges })).toEqual({ count: 2, capped: false });
  });

  it("counts routes from every pinned lever, not just the first", () => {
    const edges = [sized("a.x", "c.z"), sized("b.x", "c.z")];
    expect(countPathsTo("c.z", ["a.x", "b.x"], { edges })).toEqual({ count: 2, capped: false });
  });

  it("returns nothing when there is no tree or no lever to walk from", () => {
    expect(countPathsTo("c.z", ["a.x"], undefined)).toEqual({ count: 0, capped: false });
    expect(countPathsTo("c.z", [], { edges: [sized("a.x", "c.z")] })).toEqual({
      count: 0,
      capped: false
    });
  });

  // The metric tree is a DAG, and a cycle in a hand-written `drivers:` graph
  // must not hang the panel.
  it("terminates on a cycle", () => {
    const edges = [sized("a.x", "b.y"), sized("b.y", "a.x"), sized("b.y", "c.z")];
    expect(countPathsTo("c.z", ["a.x"], { edges })).toEqual({ count: 1, capped: false });
  });

  // The walk stops at eight, so past that the count is a floor. It has to say
  // so, or a measure reached by sixteen routes reads as reached by eight.
  it("reports the cap as a floor rather than a total", () => {
    // Four independent two-way splits between the lever and the target: every
    // combination is a distinct simple path, so 2^4 = 16 routes.
    const edges: MetricEdge[] = [];
    const stages = ["a.x", "s1", "s2", "s3", "s4", "c.z"];
    for (let i = 0; i < stages.length - 1; i++) {
      edges.push(sized(stages[i], `via.${i}.left`), sized(`via.${i}.left`, stages[i + 1]));
      edges.push(sized(stages[i], `via.${i}.right`), sized(`via.${i}.right`, stages[i + 1]));
    }
    const result = countPathsTo("c.z", ["a.x"], { edges });
    expect(result.count).toBe(8);
    expect(result.capped).toBe(true);
  });

  // The boundary the cap gets wrong if it stops AT the cap rather than one
  // past it: the walk halts identically on "exactly eight" and "eight so far",
  // so `capped` set on the act of stopping renders a graph with exactly eight
  // routes as "8+". Fan `n` parallel edges into the target to hit each side.
  it.each([
    [8, false],
    [9, true]
  ])("reports %i routes with capped=%s", (routes, capped) => {
    const edges = Array.from({ length: routes }, (_, i) => [
      sized("a.x", `mid.${i}`),
      sized(`mid.${i}`, "c.z")
    ]).flat();
    expect(countPathsTo("c.z", ["a.x"], { edges })).toEqual({
      count: Math.min(routes, 8),
      capped
    });
  });

  // The panel says the figure sums these routes. A route through a driver edge
  // nothing sized reaches the measure but contributes no magnitude, so counting
  // it would overstate what was added up.
  it("skips a route whose driver edge carries no magnitude", () => {
    const edges = [
      sized("a.x", "left.y"),
      sized("left.y", "c.z"),
      // Same shape, but the second hop was never sized — declared nor fitted.
      sized("a.x", "right.y"),
      edge("right.y", "c.z")
    ];
    expect(countPathsTo("c.z", ["a.x"], { edges })).toEqual({ count: 1, capped: false });
  });

  it("counts a route sized only by the baseline's fit", () => {
    const edges = [sized("a.x", "left.y"), sized("left.y", "c.z"), edge("a.x", "c.z")];
    const fitted: FittedDriver[] = [{ from: "a.x", to: "c.z", coefficient: 0.2, n: 400 }];
    expect(countPathsTo("c.z", ["a.x"], { edges }, fitted)).toEqual({ count: 2, capped: false });
  });

  // A component edge's content is arithmetic, not a fitted magnitude, so it
  // must not be filtered out for lacking a coefficient.
  it("counts a route through a component edge", () => {
    const edges = [edge("a.x", "c.z", { kind: "component", sign: 1 })];
    expect(countPathsTo("c.z", ["a.x"], { edges })).toEqual({ count: 1, capped: false });
  });
});
