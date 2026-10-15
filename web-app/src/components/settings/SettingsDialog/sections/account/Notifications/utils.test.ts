import { describe, expect, it } from "vitest";
import { deviceCount, humanizeKind, kindLabel } from "./utils";

describe("humanizeKind", () => {
  it("reads a kind as words, first letter up", () => {
    expect(humanizeKind("list_overdue")).toBe("List overdue");
    expect(humanizeKind("order.created")).toBe("Order created");
    expect(humanizeKind("temp-alarm")).toBe("Temp alarm");
  });

  it("leaves the app's own capitals alone", () => {
    expect(humanizeKind("SLA_breach")).toBe("SLA breach");
  });

  it("falls back to the raw kind when there are no words", () => {
    expect(humanizeKind("__")).toBe("__");
  });
});

describe("deviceCount", () => {
  it("counts devices in a few words", () => {
    expect(deviceCount(0)).toBe("Not on any device yet");
    expect(deviceCount(1)).toBe("Reaches 1 device");
    expect(deviceCount(3)).toBe("Reaches 3 devices");
  });
});

describe("kindLabel", () => {
  it("names Oxygen's own kinds", () => {
    expect(kindLabel("oxy.billing_past_due")).toBe("Billing past due");
    expect(kindLabel("oxy.workspace_ready")).toBe("Workspace ready");
    expect(kindLabel("oxy.run_failed")).toBe("Run failed");
    expect(kindLabel("oxy.anomaly")).toBe("Anomalies");
  });

  it("humanizes an Oxygen kind it does not know yet, without the prefix", () => {
    expect(kindLabel("oxy.seat_limit_near")).toBe("Seat limit near");
  });

  it("humanizes an app's kind as before", () => {
    expect(kindLabel("list_overdue")).toBe("List overdue");
  });
});
