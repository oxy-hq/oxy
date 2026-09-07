import { describe, expect, it } from "vitest";
import { measureDescription, measureTitle } from "./measureTitle";

describe("measureTitle", () => {
  it("titles from the measure name, not the description airlayer put in `label`", () => {
    // How a documented measure actually arrives: airlayer builds `label` as
    // `description ?? name`, so both fields carry the sentence.
    const node = {
      measure: "net_revenue",
      label: "Total revenue recognised across all completed orders, net of refunds",
      description: "Total revenue recognised across all completed orders, net of refunds"
    };
    expect(measureTitle(node)).toBe("net_revenue");
  });

  it("shows the name verbatim — it is the identifier written in the .view.yml", () => {
    expect(measureTitle({ measure: "order_count", label: "order_count" })).toBe("order_count");
  });

  it("falls back to the label when a node has no measure name", () => {
    expect(measureTitle({ measure: "", label: "revenue" })).toBe("revenue");
  });
});

describe("measureDescription", () => {
  it("returns the description when it adds something the title didn't", () => {
    const node = {
      measure: "net_revenue",
      label: "Revenue net of refunds",
      description: "Revenue net of refunds"
    };
    expect(measureDescription(node)).toBe("Revenue net of refunds");
  });

  it("drops a description that only restates the name", () => {
    // Otherwise the definition panel prints the same word as heading and body.
    const node = { measure: "net_revenue", label: "net_revenue", description: "net_revenue" };
    expect(measureDescription(node)).toBeNull();
  });

  it("returns null when there is no description", () => {
    expect(measureDescription({ measure: "revenue", label: "revenue" })).toBeNull();
  });
});
