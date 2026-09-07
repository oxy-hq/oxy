import { describe, expect, it } from "vitest";
import type { AppAccessSummary } from "@/types/appAccess";
import type { FrontlineWorker } from "@/types/frontline";
import { appForReturnTo, pinProblem, sameIds, workerStanding } from "./utils";

const NOW = Date.parse("2026-09-07T12:00:00Z");

const worker = (over: Partial<FrontlineWorker> = {}): FrontlineWorker => ({
  user_id: "u1",
  name: "Nia Okafor",
  identifier: "nia.o",
  status: "active",
  created_at: "2026-09-07T10:00:00Z",
  apps: [],
  locked_until: null,
  assignments: [],
  ...over
});

describe("workerStanding", () => {
  it("is active with no lockout, and once a lockout has passed", () => {
    expect(workerStanding(worker(), NOW)).toBe("active");
    expect(workerStanding(worker({ locked_until: new Date(NOW - 1).toISOString() }), NOW)).toBe(
      "active"
    );
  });

  it("is locked while a lockout is running", () => {
    expect(
      workerStanding(worker({ locked_until: new Date(NOW + 60_000).toISOString() }), NOW)
    ).toBe("locked");
  });

  it("reads suspended over a running lockout — the standing an admin chose wins", () => {
    const both = worker({
      status: "suspended",
      locked_until: new Date(NOW + 60_000).toISOString()
    });
    expect(workerStanding(both, NOW)).toBe("suspended");
  });
});

describe("pinProblem", () => {
  it("accepts 4 to 8 digits that match", () => {
    expect(pinProblem("2468", "2468")).toBeNull();
    expect(pinProblem("12345678", "12345678")).toBeNull();
  });

  it("names the rule that failed", () => {
    expect(pinProblem("123", "123")).toMatch(/4 to 8 digits/);
    expect(pinProblem("123456789", "123456789")).toMatch(/4 to 8 digits/);
    expect(pinProblem("12a4", "12a4")).toMatch(/4 to 8 digits/);
    expect(pinProblem("2468", "2469")).toMatch(/don't match/);
  });
});

describe("appForReturnTo", () => {
  const apps = [
    { id: "a1", slug: "store-ops", name: "Store Ops" },
    { id: "a2", slug: "inventory", name: "Inventory" }
  ] as AppAccessSummary[];

  it("resolves an app by path on any host of the deployment", () => {
    expect(
      appForReturnTo(apps, "poke", "https://poke.oxygen-hq.com/customer-apps/poke/store-ops/")?.id
    ).toBe("a1");
    expect(
      appForReturnTo(apps, "poke", "http://127.0.0.1:5173/customer-apps/poke/inventory")?.id
    ).toBe("a2");
  });

  it("resolves nothing for another org, an unknown app, no URL, or garbage", () => {
    expect(
      appForReturnTo(apps, "poke", "https://x/customer-apps/other/store-ops/")
    ).toBeUndefined();
    expect(appForReturnTo(apps, "poke", "https://x/customer-apps/poke/missing/")).toBeUndefined();
    expect(appForReturnTo(apps, "poke", null)).toBeUndefined();
    expect(appForReturnTo(apps, "poke", "not a url")).toBeUndefined();
  });
});

describe("sameIds", () => {
  it("ignores order and catches any difference", () => {
    expect(sameIds(["a", "b"], ["b", "a"])).toBe(true);
    expect(sameIds(["a", "b"], ["a"])).toBe(false);
    expect(sameIds(["a"], ["b"])).toBe(false);
    expect(sameIds([], [])).toBe(true);
  });
});
