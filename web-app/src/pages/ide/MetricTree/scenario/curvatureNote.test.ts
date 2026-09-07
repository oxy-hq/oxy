import { describe, expect, it } from "vitest";
import type { FittedDriver } from "@/types/metricTree";
import { curvatureNote } from "./curvatureNote";

function fitWithT(t_stats?: (number | null)[]): FittedDriver {
  return {
    from: "store_days.discount_depth",
    to: "store_days.promo_margin",
    coefficient: 0.8353,
    t_stats
  };
}

describe("curvatureNote", () => {
  // The peak is `vertex·s₁/s₂ − 1`, a ratio built from a difference of two close
  // quantities, so a curvature sitting on the |t| >= 2 gate moves it a long way.
  // The panel printed the peak with nothing to weigh it by.
  it("warns when the curvature is only just resolved", () => {
    const note = curvatureNote(fitWithT([53.7, 2.4]));
    expect(note).toMatch(/only just resolved/);
    expect(note).toContain("2.4");
  });

  it("says so when the curvature is comfortably resolved", () => {
    expect(curvatureNote(fitWithT([53.7, 33.5]))).toMatch(/well resolved/);
  });

  // The headline `t_stat` is the SLOPE's and says nothing about whether the
  // shape turns, so a fit carrying no per-term array has nothing to report —
  // and must not fall back to a number that answers a different question.
  it("says nothing without a per-term t for the curvature", () => {
    expect(curvatureNote(fitWithT())).toBe("");
    expect(curvatureNote(fitWithT([53.7]))).toBe("");
    expect(curvatureNote(fitWithT([53.7, null]))).toBe("");
    expect(curvatureNote(fitWithT([53.7, Number.NaN]))).toBe("");
  });

  // Sign is not the question — the gate is on |t|, and a curvature of -33.5 is
  // a downward turn measured just as sharply as an upward one.
  it("reads the magnitude, not the sign", () => {
    expect(curvatureNote(fitWithT([53.7, -33.5]))).toMatch(/well resolved/);
    expect(curvatureNote(fitWithT([53.7, -2.4]))).toMatch(/only just resolved/);
  });
});
