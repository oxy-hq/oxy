import { describe, expect, it } from "vitest";
import { appWindowFor, badgeLabel } from "./utils";

describe("badgeLabel", () => {
  it("is nothing at zero", () => {
    expect(badgeLabel(0)).toBeNull();
    expect(badgeLabel(-1)).toBeNull();
  });

  it("is the count up to 99, then 99+", () => {
    expect(badgeLabel(1)).toBe("1");
    expect(badgeLabel(99)).toBe("99");
    expect(badgeLabel(100)).toBe("99+");
    expect(badgeLabel(4321)).toBe("99+");
  });
});

describe("appWindowFor", () => {
  it("opens a link into a custom app in that app's own org-scoped tab", () => {
    expect(appWindowFor("/customer-apps/acme/store-ops/lists/42")).toBe("oxy-app-acme-store-ops");
    expect(appWindowFor("/customer-apps/acme/store-ops/")).toBe("oxy-app-acme-store-ops");
    expect(appWindowFor("/customer-apps/acme/store-ops")).toBe("oxy-app-acme-store-ops");
    expect(appWindowFor("/customer-apps/acme/store-ops?tab=overdue")).toBe(
      "oxy-app-acme-store-ops"
    );
  });

  it("keeps a path inside the product in this tab", () => {
    expect(appWindowFor("/acme/workspaces/ws-1/threads")).toBeNull();
    expect(appWindowFor("/work/1")).toBeNull();
    // Not an app: no app segment.
    expect(appWindowFor("/customer-apps/acme")).toBeNull();
  });
});
