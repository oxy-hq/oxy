import { describe, expect, it } from "vitest";
import type { KindNode, SourceNode } from "@/types/notifications";
import { deviceCount, prefRows } from "./utils";

describe("deviceCount", () => {
  it("counts devices in a few words", () => {
    expect(deviceCount(0)).toBe("Not on any device yet");
    expect(deviceCount(1)).toBe("Reaches 1 device");
    expect(deviceCount(3)).toBe("Reaches 3 devices");
  });
});

const kind = (over: Partial<KindNode> & { kind: string }): KindNode => ({
  label: over.kind,
  description: null,
  level: "active",
  critical: false,
  channels: ["push", "email", "slack"],
  muted: [],
  actions: [],
  unread: 0,
  ...over
});

const source = (over: Partial<SourceNode> = {}): SourceNode => ({
  source: "app",
  app: { id: "app-1", slug: "store-ops", name: "Store Ops", org_slug: "acme" },
  label: "Store Ops",
  muted: [],
  unread: 0,
  categories: [
    {
      path: "operations",
      label: "Store operations",
      description: null,
      muted: ["email"],
      unread: 0,
      kinds: [kind({ kind: "list_overdue", label: "Overdue lists" })],
      categories: [
        {
          path: "operations/food-safety",
          label: "Food safety",
          description: "Temperature logs and checks",
          muted: [],
          unread: 0,
          categories: [],
          kinds: [
            kind({
              kind: "temp_alarm",
              label: "Temperature alarm",
              critical: true,
              level: "critical"
            })
          ]
        }
      ]
    }
  ],
  ...over
});

describe("prefRows", () => {
  it("lists the source, then each category with its kinds before its own categories", () => {
    expect(prefRows(source()).map((r) => [r.key, r.depth])).toEqual([
      ["source", 0],
      ["cat-operations", 1],
      ["kind-list_overdue", 2],
      ["cat-operations/food-safety", 2],
      ["kind-temp_alarm", 3]
    ]);
  });

  it("writes a source as scope '', a category as its path, a kind as the kind", () => {
    const rows = prefRows(source());
    expect(rows[0].target).toEqual({ scope: "" });
    expect(rows[1].target).toEqual({ scope: "operations" });
    expect(rows[2].target).toEqual({ kind: "list_overdue" });
  });

  it("names the row whose mute covers each child, per channel", () => {
    const rows = prefRows(source());
    expect(rows[1].mutedBy).toEqual({});
    expect(rows[2].mutedBy).toEqual({ email: "Store operations" });
    expect(rows[3].mutedBy).toEqual({ email: "Store operations" });
  });

  it("names the highest muting row when two above both mute", () => {
    const rows = prefRows(source({ muted: ["email", "push"] }));
    expect(rows[2].mutedBy).toEqual({ email: "Store Ops", push: "Store Ops" });
  });

  it("locks a subtree of only critical kinds, and not one with any other kind", () => {
    const rows = prefRows(source());
    expect(rows.find((r) => r.key === "cat-operations/food-safety")?.critical).toBe(true);
    expect(rows.find((r) => r.key === "cat-operations")?.critical).toBe(false);
    expect(rows.find((r) => r.key === "kind-temp_alarm")?.critical).toBe(true);
  });

  it("offers a parent only the channels something under it is sent on", () => {
    const rows = prefRows({
      ...source(),
      categories: [
        {
          path: "runs",
          label: "Automations and pipelines",
          description: null,
          muted: [],
          unread: 0,
          categories: [],
          kinds: [kind({ kind: "oxy.run_failed", channels: ["push", "email"] })]
        }
      ]
    });
    expect(rows.map((r) => r.channels)).toEqual([
      ["push", "email"],
      ["push", "email"],
      ["push", "email"]
    ]);
  });
});
