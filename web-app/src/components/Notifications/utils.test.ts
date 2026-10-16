import { describe, expect, it } from "vitest";
import type { InboxItem } from "@/types/notifications";
import { appWindowFor, sourceLine } from "./utils";

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

describe("sourceLine", () => {
  const item = (over: Partial<InboxItem>): InboxItem => ({
    id: "n",
    org_id: "o",
    kind: "temp_alarm",
    created_at: "2026-10-14T11:55:00Z",
    read_at: null,
    title: "t",
    body: null,
    href: null,
    link: null,
    app: { id: "a", slug: "store-ops", name: "Store Ops" },
    ...over
  });

  it("names the source and the category's own label", () => {
    expect(
      sourceLine(
        item({
          category: {
            path: "operations/food-safety",
            labels: ["Store operations", "Food safety"]
          }
        })
      )
    ).toBe("Store Ops · Food safety");
  });

  it("is Oxygen for a platform notice", () => {
    expect(
      sourceLine(item({ app: null, category: { path: "billing", labels: ["Billing"] } }))
    ).toBe("Oxygen · Billing");
  });

  it("leaves out the implicit General category", () => {
    expect(sourceLine(item({ category: { path: "general", labels: ["General"] } }))).toBe(
      "Store Ops"
    );
  });

  it("is just the source from a server that sends no category", () => {
    expect(sourceLine(item({}))).toBe("Store Ops");
    expect(sourceLine(item({ app: null }))).toBe("Oxygen");
  });
});
