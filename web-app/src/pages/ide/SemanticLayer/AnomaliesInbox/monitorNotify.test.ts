import { describe, expect, it } from "vitest";
import type { MonitorNotify } from "@/types/metricAnomalies";
import { announcedSeverity, notifiedPeople } from "./monitorNotify";

describe("announcedSeverity", () => {
  it("reads the floor as a floor", () => {
    const at = (min_severity: "low" | "medium" | "high") =>
      announcedSeverity({ slack_channel: "C0123ABCDEF", min_severity });
    expect(at("high")).toBe("at high severity");
    expect(at("medium")).toBe("at medium severity or above");
    expect(at("low")).toBe("of any severity");
  });
});

describe("notifiedPeople", () => {
  const block = (people: MonitorNotify["people"]): MonitorNotify => ({
    people,
    min_severity: "high"
  });

  it("names each kind of entry in a few words", () => {
    expect(notifiedPeople(block([{ email: "dana@acme.com" }]))).toBe("dana@acme.com");
    expect(notifiedPeople(block([{ role: "owner" }]))).toBe("the owners");
    expect(notifiedPeople(block([{ role: "admin" }]))).toBe("the admins");
    expect(notifiedPeople(block([{ role: "member" }]))).toBe("every member");
    expect(notifiedPeople(block([{ team: "Finance" }]))).toBe("team Finance");
  });

  it("lists up to three, then counts the rest", () => {
    expect(notifiedPeople(block([{ email: "dana@acme.com" }, { role: "admin" }]))).toBe(
      "dana@acme.com and the admins"
    );
    expect(
      notifiedPeople(block([{ email: "dana@acme.com" }, { role: "admin" }, { team: "Finance" }]))
    ).toBe("dana@acme.com, the admins and team Finance");
    expect(
      notifiedPeople(
        block([
          { email: "dana@acme.com" },
          { email: "lee@acme.com" },
          { role: "admin" },
          { team: "Finance" }
        ])
      )
    ).toBe("dana@acme.com, lee@acme.com and 2 more");
  });

  it("is null for a block that names nobody", () => {
    expect(notifiedPeople(block(undefined))).toBeNull();
    expect(notifiedPeople(block(null))).toBeNull();
    expect(notifiedPeople(block([]))).toBeNull();
    expect(notifiedPeople({ slack_channel: "C0123ABCDEF", min_severity: "high" })).toBeNull();
  });
});
