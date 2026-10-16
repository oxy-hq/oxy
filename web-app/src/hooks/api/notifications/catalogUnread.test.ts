import { describe, expect, it } from "vitest";
import type { CategoryNode, KindNode, NotificationCatalog } from "@/types/notifications";
import { withFilterRead, withItemRead } from "./catalogUnread";

const kind = (name: string, unread: number): KindNode => ({
  kind: name,
  label: name,
  description: null,
  level: "active",
  critical: false,
  channels: ["push", "email", "slack"],
  muted: [],
  actions: [],
  unread
});

const category = (
  path: string,
  unread: number,
  kinds: KindNode[],
  categories: CategoryNode[] = []
): CategoryNode => ({ path, label: path, description: null, muted: [], unread, kinds, categories });

/** Store Ops 4 (operations 4: list_overdue 1, food-safety 3: temp_alarm 3), Oxygen 2, Books 5. */
const catalog = (): NotificationCatalog => ({
  reach: { push_devices: 1, email: true },
  sources: [
    {
      source: "app",
      app: { id: "app-1", slug: "store-ops", name: "Store Ops", org_slug: "acme" },
      label: "Store Ops",
      muted: [],
      unread: 4,
      categories: [
        category(
          "operations",
          4,
          [kind("list_overdue", 1)],
          [category("operations/food-safety", 3, [kind("temp_alarm", 3)])]
        )
      ]
    },
    {
      source: "oxygen",
      app: null,
      label: "Oxygen",
      muted: [],
      unread: 2,
      categories: [category("runs", 2, [kind("oxy.run_failed", 2)])]
    },
    {
      source: "app",
      app: { id: "app-2", slug: "books", name: "Books", org_slug: "acme" },
      label: "Books",
      muted: [],
      unread: 5,
      categories: [category("general", 5, [kind("temp_alarm", 5)])]
    }
  ]
});

/** Every node's unread, root to leaf, for one source. */
function counts(c: NotificationCatalog, index: number): Record<string, number> {
  const source = c.sources[index];
  const out: Record<string, number> = { "": source.unread };
  const walk = (node: CategoryNode) => {
    out[node.path] = node.unread;
    for (const k of node.kinds) out[`${node.path}#${k.kind}`] = k.unread;
    node.categories.forEach(walk);
  };
  source.categories.forEach(walk);
  return out;
}

const STORE_OPS = { id: "app-1", slug: "store-ops", name: "Store Ops" };

describe("withItemRead", () => {
  it("takes one off the item's source, every category on its path and its kind", () => {
    const next = withItemRead(catalog(), { app: STORE_OPS, kind: "temp_alarm", read_at: null });
    expect(counts(next, 0)).toEqual({
      "": 3,
      operations: 3,
      "operations#list_overdue": 1,
      "operations/food-safety": 2,
      "operations/food-safety#temp_alarm": 2
    });
    // The same kind in another app, and Oxygen, are untouched.
    expect(counts(next, 2)).toEqual(counts(catalog(), 2));
    expect(counts(next, 1)).toEqual(counts(catalog(), 1));
  });

  it("reads Oxygen's notices against Oxygen", () => {
    const next = withItemRead(catalog(), { app: null, kind: "oxy.run_failed", read_at: null });
    expect(counts(next, 1)).toEqual({ "": 1, runs: 1, "runs#oxy.run_failed": 1 });
    expect(counts(next, 0)).toEqual(counts(catalog(), 0));
  });

  it("changes nothing for an item already read, or a count already at zero", () => {
    const before = catalog();
    expect(
      withItemRead(before, { app: STORE_OPS, kind: "temp_alarm", read_at: "2026-10-14T00:00:00Z" })
    ).toBe(before);
    let c = catalog();
    for (let i = 0; i < 5; i++) {
      c = withItemRead(c, { app: STORE_OPS, kind: "list_overdue", read_at: null });
    }
    expect(counts(c, 0)).toMatchObject({ "": 3, operations: 3, "operations#list_overdue": 0 });
  });
});

describe("withFilterRead", () => {
  it("zeroes everything for the whole inbox", () => {
    const next = withFilterRead(catalog(), {});
    expect(next?.sources.map((s) => s.unread)).toEqual([0, 0, 0]);
    expect(Object.values(counts(next as NotificationCatalog, 0)).every((n) => n === 0)).toBe(true);
  });

  it("zeroes one source and leaves the others", () => {
    const next = withFilterRead(catalog(), { source: "oxygen" }) as NotificationCatalog;
    expect(next.sources.map((s) => s.unread)).toEqual([4, 0, 5]);
    expect(counts(next, 1)).toEqual({ "": 0, runs: 0, "runs#oxy.run_failed": 0 });
  });

  it("zeroes a category's subtree and takes it off its ancestors", () => {
    const next = withFilterRead(catalog(), {
      source: "app-1",
      category: "operations/food-safety"
    }) as NotificationCatalog;
    expect(counts(next, 0)).toEqual({
      "": 1,
      operations: 1,
      "operations#list_overdue": 1,
      "operations/food-safety": 0,
      "operations/food-safety#temp_alarm": 0
    });
    expect(counts(next, 2)).toEqual(counts(catalog(), 2));
  });

  it("zeroes one kind of one source", () => {
    const next = withFilterRead(catalog(), {
      source: "app-1",
      kind: "temp_alarm"
    }) as NotificationCatalog;
    expect(counts(next, 0)).toMatchObject({
      "": 1,
      operations: 1,
      "operations/food-safety": 0,
      "operations/food-safety#temp_alarm": 0
    });
    expect(counts(next, 2), "the same kind of another app").toEqual(counts(catalog(), 2));
  });

  it("cannot tell what a thread covered", () => {
    expect(withFilterRead(catalog(), { source: "app-1", thread: "t-1" })).toBeNull();
  });
});
