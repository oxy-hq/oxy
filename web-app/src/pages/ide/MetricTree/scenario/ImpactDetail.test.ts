import { describe, expect, it } from "vitest";
import type { FittedDriver } from "@/types/metricTree";
import { inferredHelp } from "./ImpactDetail";
import type { TraceHop } from "./propagationTrace";

function hop(fit?: FittedDriver): TraceHop {
  return {
    from: "orders.checks",
    to: "orders.net_sales",
    kind: "driver",
    form: "quadratic",
    formDeclared: false,
    coefficientSource: fit ? "fitted" : "none",
    fit
  };
}

function fitWith(candidates?: FittedDriver["candidates"]): FittedDriver {
  return { from: "orders.checks", to: "orders.net_sales", coefficient: 0.84, n: 180, candidates };
}

describe("inferredHelp", () => {
  // "The engine chose quadratic" is not checkable; "it beat linear by 945" is.
  // That is the whole reason the fit carries `candidates` — a comparable score
  // across shapes turns a choice into an argument.
  it("names the runner-up and the margin it was beaten by", () => {
    const help = inferredHelp(
      hop(
        fitWith([
          { form: "quadratic", aic: 100, all_terms_significant: true },
          { form: "linear", aic: 1045, all_terms_significant: true }
        ])
      )
    );
    expect(help).toContain("beat linear by 945");
    expect(help).toContain("2 shapes");
  });

  // A candidate that failed the significance gate was never eligible however
  // good its score, so naming it as the thing that was beaten would describe a
  // contest that did not happen.
  it("ignores a candidate that never cleared the significance gate", () => {
    const help = inferredHelp(
      hop(
        fitWith([
          { form: "quadratic", aic: 100, all_terms_significant: true },
          { form: "linear", aic: 1045, all_terms_significant: true },
          // The best score in the list, and ineligible.
          { form: "cubic", aic: 40, all_terms_significant: false }
        ])
      )
    );
    expect(help).toContain("beat linear by 945");
    expect(help).not.toContain("cubic");
  });

  // Lowest-AIC-wins is NOT the engine's rule: within a 10-AIC band the
  // fewest-term shape takes it, because a richer nested shape can reproduce a
  // simpler one exactly. Anchoring on the best score instead of on the shape
  // actually displayed produced a sentence naming two shapes, neither of them
  // the one on the badge.
  it("anchors on the displayed shape when a richer one scored better", () => {
    const help = inferredHelp(
      hop(
        fitWith([
          // Better score, more terms — loses the dead heat to `quadratic`.
          { form: "cubic", aic: 97.8, all_terms_significant: true },
          { form: "quadratic", aic: 100, all_terms_significant: true },
          { form: "linear", aic: 1045, all_terms_significant: true }
        ])
      )
    );
    expect(help).toContain("within 2.20 AIC of cubic");
    expect(help).toContain("dead heat");
    // The losing-by-a-nose shape must never be described as beaten.
    expect(help).not.toContain("beat cubic");
  });

  // Under the documented rule a chosen shape is never more than 10 AIC off the
  // best — but the point of anchoring on `hop.form` was to stop the panel
  // re-deriving the engine's selection, so a payload that breaks the rule must
  // not print a number above 10 inside a sentence claiming it is under 10.
  it("says nothing when the margin is outside the dead-heat band", () => {
    const help = inferredHelp(
      hop(
        fitWith([
          { form: "cubic", aic: 50, all_terms_significant: true },
          { form: "quadratic", aic: 100, all_terms_significant: true }
        ])
      )
    );
    expect(help).not.toContain("dead heat");
    expect(help).not.toContain("50");
  });

  // The displayed shape is not among the eligible candidates: nothing here can
  // be said about a badge whose score is absent, and picking some other row's
  // margin would attribute it to the wrong shape.
  it("says only what it knows when the displayed shape is not a candidate", () => {
    const help = inferredHelp(
      hop(
        fitWith([
          { form: "cubic", aic: 97.8, all_terms_significant: true },
          { form: "linear", aic: 1045, all_terms_significant: true }
        ])
      )
    );
    expect(help).toBe(
      "The shape was measured from history rather than declared, so it can change as the window moves."
    );
  });

  // A declared shape has no candidate list, and a fit that scored only one
  // eligible shape has nothing to have beaten. Both must fall back to the plain
  // sentence rather than assemble a comparison out of nothing.
  it("says only what it knows when there is no runner-up", () => {
    const base = "measured from history";
    expect(inferredHelp(hop(fitWith()))).toContain(base);
    expect(inferredHelp(hop(fitWith([])))).toContain(base);
    expect(inferredHelp(hop())).toContain(base);
    expect(
      inferredHelp(hop(fitWith([{ form: "quadratic", aic: 100, all_terms_significant: true }])))
    ).not.toContain("beat");
  });
});
