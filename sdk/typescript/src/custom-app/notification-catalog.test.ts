// The notification catalogue's rules, case by case — the cases for every copy
// of `notification-catalog.ts` (`oxyc` and the Vite plugin carry byte copies,
// pinned by their own sync tests). Each rule mirrors the server's publish check;
// the one difference is on purpose: this lists every problem, not the first.

import { describe, expect, it } from "vitest";
import {
  type CatalogFinding,
  checkNotifications,
  humanizeNotificationName,
  isValidCategoryPath,
  resolveNotificationKind
} from "./notification-catalog";

/** The worked example: valid, and silent. Loosely typed — each case breaks a field. */
// biome-ignore lint/suspicious/noExplicitAny: a manifest as JSON holds it, wrong values included
function manifest(): Record<string, any> {
  return {
    functions: {
      lists: { notify: { kinds: { list_overdue: {}, temp_alarm: {} } } },
      "ack-alarm": { route: true }
    },
    notifications: {
      categories: {
        operations: { label: "Store operations" },
        "operations/food-safety": {
          label: "Food safety",
          description: "Temperature logs and checks"
        }
      },
      kinds: {
        list_overdue: {
          category: "operations",
          label: "Overdue lists",
          level: "time_sensitive",
          actions: [{ id: "open", label: "Open list" }]
        },
        temp_alarm: {
          category: "operations/food-safety",
          label: "Temperature alarm",
          level: "critical",
          actions: [{ id: "ack", label: "Acknowledge", function: "ack-alarm" }]
        }
      }
    }
  };
}

const errors = (m: unknown): CatalogFinding[] =>
  checkNotifications(m).filter((f) => f.level === "error");
const errorPaths = (m: unknown): string[] => errors(m).map((f) => f.path);
const warnings = (m: unknown): string[] =>
  checkNotifications(m)
    .filter((f) => f.level === "warning")
    .map((f) => `${f.path}: ${f.message}`);

describe("checkNotifications — clean", () => {
  it("says nothing about the worked example, a manifest without the block, or no manifest", () => {
    expect(checkNotifications(manifest())).toEqual([]);
    expect(checkNotifications({ functions: {} })).toEqual([]);
    expect(checkNotifications(undefined)).toEqual([]);
  });

  it("never throws, whatever it is given", () => {
    for (const m of [null, 7, "x", [], { functions: [] }, { notifications: null }]) {
      expect(() => checkNotifications(m)).not.toThrow();
    }
  });
});

describe("checkNotifications — category paths", () => {
  it("refuses a malformed path, as a declaration and as a kind's category", () => {
    const m = manifest();
    m.notifications.categories["Store Ops"] = {};
    m.notifications.kinds.list_overdue.category = "ops//x";
    expect(errors(m)).toEqual([
      expect.objectContaining({
        path: "notifications.categories.Store Ops",
        message: expect.stringContaining('Category "Store Ops" is not a valid path')
      }),
      expect.objectContaining({
        path: "notifications.kinds.list_overdue.category",
        message: expect.stringContaining('Category "ops//x" is not a valid path')
      })
    ]);
  });

  it("refuses a path deeper than three, saying how deep", () => {
    const m = manifest();
    m.notifications.categories["a/b/c/d"] = {};
    expect(errors(m)).toEqual([
      {
        level: "error",
        path: "notifications.categories.a/b/c/d",
        message: 'Category "a/b/c/d" is 4 levels deep; at most 3 are allowed.'
      }
    ]);
  });

  it("calls a malformed path malformed before it calls it deep", () => {
    const m = manifest();
    m.notifications.categories["A/b/c/d"] = {};
    expect(errors(m)[0]?.message).toContain("is not a valid path");
  });

  it("accepts 1–3 segments of [a-z0-9][a-z0-9_-]{0,31}, and nothing else", () => {
    for (const ok of ["operations", "operations/food-safety", "a/b/c", "0ps/x_y", "x".repeat(32)]) {
      expect(isValidCategoryPath(ok), ok).toBe(true);
    }
    for (const bad of [
      "",
      "Operations",
      "a//b",
      "/a",
      "a/",
      "-a",
      "a b",
      "a/b/c/d",
      "x".repeat(33)
    ]) {
      expect(isValidCategoryPath(bad), bad).toBe(false);
    }
  });
});

describe("checkNotifications — a kind's category", () => {
  it("refuses a category nobody declared", () => {
    const m = manifest();
    m.notifications.kinds.list_overdue.category = "finance";
    expect(errors(m)).toEqual([
      {
        level: "error",
        path: "notifications.kinds.list_overdue.category",
        message:
          'Kind "list_overdue" names the category "finance", which notifications.categories does not declare.'
      }
    ]);
  });

  it("accepts a parent a declared path implies, and general, which always exists", () => {
    const m = manifest();
    m.notifications.categories = { "a/b": {} };
    m.notifications.kinds.list_overdue.category = "a";
    m.notifications.kinds.temp_alarm.category = "general";
    expect(errors(m)).toEqual([]);
  });

  it("refuses a category that is not a string", () => {
    const m = manifest();
    m.notifications.kinds.list_overdue.category = ["operations"];
    expect(errorPaths(m)).toEqual(["notifications.kinds.list_overdue.category"]);
  });
});

describe("checkNotifications — level", () => {
  it("refuses a level outside the four, naming them", () => {
    const m = manifest();
    m.notifications.kinds.list_overdue.level = "urgent";
    expect(errors(m)).toEqual([
      {
        level: "error",
        path: "notifications.kinds.list_overdue.level",
        message:
          'Kind "list_overdue" has level "urgent"; expected one of passive, active, time_sensitive, critical.'
      }
    ]);
  });
});

describe("checkNotifications — actions", () => {
  it("refuses more than three", () => {
    const m = manifest();
    m.notifications.kinds.list_overdue.actions = [0, 1, 2, 3].map((i) => ({
      id: `a${i}`,
      label: "Go"
    }));
    expect(errors(m)).toEqual([
      expect.objectContaining({
        path: "notifications.kinds.list_overdue.actions",
        message: 'Kind "list_overdue" declares 4 actions; at most 3 are allowed.'
      })
    ]);
  });

  it("refuses an action naming a function the manifest does not declare", () => {
    const m = manifest();
    m.notifications.kinds.temp_alarm.actions = [
      { id: "ack", label: "Acknowledge", function: "ack-it" }
    ];
    expect(errors(m)).toEqual([
      {
        level: "error",
        path: "notifications.kinds.temp_alarm.actions[0].function",
        message:
          'Action "ack" of kind "temp_alarm" calls the function "ack-it", which this manifest does not declare under functions.'
      }
    ]);
  });

  it("refuses a bad id, a blank or long label, a duplicate id and a non-boolean destructive", () => {
    const cases: [unknown[], string][] = [
      [[{ id: "Ack", label: "Acknowledge" }], "needs an `id`"],
      [[{ id: "ack", label: "" }], "needs a `label` of 1–24 characters"],
      [[{ id: "ack", label: "x".repeat(25) }], "needs a `label` of 1–24 characters"],
      [
        [
          { id: "ack", label: "A" },
          { id: "ack", label: "B" }
        ],
        'declares the action "ack" twice'
      ],
      [[{ id: "ack", label: "A", destructive: "yes" }], "`destructive` that is not true or false"],
      [["ack"], "must be an object"]
    ];
    for (const [actions, why] of cases) {
      const m = manifest();
      m.notifications.kinds.temp_alarm.actions = actions;
      const found = errors(m);
      expect(found, JSON.stringify(actions)).toHaveLength(1);
      expect(found[0]?.message, JSON.stringify(actions)).toContain(why);
    }
  });

  it("counts a 24-character label in characters, not UTF-16 units", () => {
    const m = manifest();
    m.notifications.kinds.temp_alarm.actions = [{ id: "ack", label: "🔥".repeat(24) }];
    expect(errors(m)).toEqual([]);
  });

  it("refuses an action that is not a list", () => {
    const m = manifest();
    m.notifications.kinds.temp_alarm.actions = { id: "ack" };
    expect(errorPaths(m)).toEqual(["notifications.kinds.temp_alarm.actions"]);
  });
});

describe("checkNotifications — kinds", () => {
  it("refuses a kind in Oxygen's namespace, in the catalogue and on a function", () => {
    const m = manifest();
    m.notifications.kinds["oxy.anomaly"] = {};
    m.functions.lists.notify.kinds["oxy.run_failed"] = {};
    expect(errors(m)).toEqual([
      {
        level: "error",
        path: "functions.lists.notify.kinds.oxy.run_failed",
        message:
          'function "lists" declares notify kind "oxy.run_failed", which starts with "oxy." — reserved for Oxygen\'s own notices; rename it.'
      },
      {
        level: "error",
        path: "notifications.kinds.oxy.anomaly",
        message:
          'Kind "oxy.anomaly" starts with "oxy.", which is reserved for Oxygen\'s own notices.'
      }
    ]);
  });

  it("refuses a kind name ctx.notify could never send, and a kind that is not an object", () => {
    const m = manifest();
    m.notifications.kinds["List Overdue"] = {};
    m.notifications.kinds.list_overdue = "operations";
    expect(errors(m)).toEqual([
      expect.objectContaining({
        path: "notifications.kinds.list_overdue",
        message: "`notifications.kinds.list_overdue` must be an object."
      }),
      expect.objectContaining({
        path: "notifications.kinds.List Overdue",
        message: expect.stringContaining("must be 1–64 lowercase letters")
      })
    ]);
  });

  it("refuses a misspelt field at every level, so a typo'd `function` is not a quiet open", () => {
    const m = manifest();
    m.notifications.kind = {};
    m.notifications.categories.operations.lable = "x";
    m.notifications.kinds.list_overdue.lable = "x";
    m.notifications.kinds.temp_alarm.actions = [{ id: "ack", label: "Ack", fucntion: "ack-alarm" }];
    expect(errors(m).map((f) => f.message)).toEqual([
      "`notifications.kind` is not a field; notifications takes categories, kinds.",
      "`notifications.categories.operations.lable` is not a field; notifications.categories.operations takes label, description.",
      "`notifications.kinds.list_overdue.lable` is not a field; notifications.kinds.list_overdue takes category, label, description, level, actions.",
      "`notifications.kinds.temp_alarm.actions[0].fucntion` is not a field; notifications.kinds.temp_alarm.actions[0] takes id, label, function, destructive."
    ]);
  });

  it("refuses a blank or long label and a long description", () => {
    const m = manifest();
    m.notifications.categories.operations.label = " ";
    m.notifications.kinds.list_overdue.label = "x".repeat(65);
    m.notifications.kinds.temp_alarm.description = "x".repeat(201);
    expect(errors(m).map((f) => f.message)).toEqual([
      "`notifications.categories.operations.label` must be a non-empty string of at most 64 characters.",
      "`notifications.kinds.list_overdue.label` must be a non-empty string of at most 64 characters.",
      "`notifications.kinds.temp_alarm.description` must be a non-empty string of at most 200 characters."
    ]);
  });

  it("refuses a block, categories or kinds that are not objects", () => {
    expect(errorPaths({ notifications: [] })).toEqual(["notifications"]);
    expect(errorPaths({ notifications: { categories: [], kinds: 1 } })).toEqual([
      "notifications.categories",
      "notifications.kinds"
    ]);
  });

  it("lists every problem in one pass, in manifest order", () => {
    const m = manifest();
    m.notifications.categories["a/b/c/d"] = {};
    m.notifications.kinds.list_overdue.level = "urgent";
    m.notifications.kinds.temp_alarm.category = "finance";
    expect(errorPaths(m)).toEqual([
      "notifications.categories.a/b/c/d",
      "notifications.kinds.list_overdue.level",
      "notifications.kinds.temp_alarm.category"
    ]);
  });
});

describe("checkNotifications — warnings", () => {
  it("warns about a kind no function sends, without refusing", () => {
    const m = manifest();
    m.notifications.kinds.stock_low = { level: "passive" };
    expect(errors(m)).toEqual([]);
    expect(warnings(m)).toEqual([
      "notifications.kinds.stock_low: no function declares it under notify.kinds, so nothing can send it yet"
    ]);
  });

  it("warns when a function's critical: true overrides the catalogue's level", () => {
    const m = manifest();
    m.functions.lists.notify.kinds.list_overdue = { critical: true };
    expect(warnings(m)).toEqual([
      "notifications.kinds.list_overdue: its level is time_sensitive but a function marks it critical: true; it is delivered as critical"
    ]);
  });
});

describe("resolveNotificationKind", () => {
  it("reads a kind's place, label, level and actions from the catalogue", () => {
    expect(resolveNotificationKind(manifest(), "temp_alarm")).toEqual({
      category: { path: "operations/food-safety", labels: ["Store operations", "Food safety"] },
      kind_label: "Temperature alarm",
      level: "critical",
      actions: [{ id: "ack", label: "Acknowledge", destructive: false, runs_function: true }]
    });
  });

  it("gives an undescribed kind the defaults: general, its name humanized, active, no actions", () => {
    expect(resolveNotificationKind({ functions: {} }, "delivery_received")).toEqual({
      category: { path: "general", labels: ["General"] },
      kind_label: "Delivery received",
      level: "active",
      actions: []
    });
    expect(resolveNotificationKind(undefined, "x").category.path).toBe("general");
  });

  it("makes a kind critical when any function says so, whatever the catalogue's level", () => {
    const m = manifest();
    m.functions.lists.notify.kinds.list_overdue = { critical: true };
    expect(resolveNotificationKind(m, "list_overdue").level).toBe("critical");
    expect(
      resolveNotificationKind(
        { functions: { f: { notify: { kinds: { k: { critical: true } } } } } },
        "k"
      ).level
    ).toBe("critical");
  });

  it("labels an implied parent by humanizing its segment", () => {
    const m = manifest();
    m.notifications.categories = { "store-ops/food_safety": { label: "Food safety" } };
    m.notifications.kinds.temp_alarm.category = "store-ops/food_safety";
    expect(resolveNotificationKind(m, "temp_alarm").category.labels).toEqual([
      "Store ops",
      "Food safety"
    ]);
  });

  it("falls back part by part, like the server's read of a live build", () => {
    const m = manifest();
    m.notifications.kinds.temp_alarm = {
      category: "Bad Path",
      label: "  Alarm  ",
      level: "urgent",
      actions: [
        { id: "Bad", label: "x" },
        { id: "a", label: " " },
        { id: "b", label: "B", function: "  " },
        { id: "c", label: "C", destructive: true },
        { id: "d", label: "D" },
        { id: "e", label: "E" }
      ]
    };
    expect(resolveNotificationKind(m, "temp_alarm")).toEqual({
      category: { path: "general", labels: ["General"] },
      kind_label: "Alarm",
      level: "active",
      actions: [
        { id: "b", label: "B", destructive: false, runs_function: false },
        { id: "c", label: "C", destructive: true, runs_function: false },
        { id: "d", label: "D", destructive: false, runs_function: false }
      ]
    });
  });

  it("never reads an entry for a reserved or malformed kind", () => {
    const m = manifest();
    m.notifications.kinds["oxy.anomaly"] = { label: "Mine now" };
    expect(resolveNotificationKind(m, "oxy.anomaly").kind_label).toBe("Oxy anomaly");
    expect(resolveNotificationKind(m, "__proto__").kind_label).toBe("Proto");
  });
});

describe("humanizeNotificationName", () => {
  it("spaces the separators and capitalizes the first letter", () => {
    expect(humanizeNotificationName("list_overdue")).toBe("List overdue");
    expect(humanizeNotificationName("food-safety")).toBe("Food safety");
    expect(humanizeNotificationName("oxy.run_failed")).toBe("Oxy run failed");
    expect(humanizeNotificationName("a__b")).toBe("A b");
    expect(humanizeNotificationName("___")).toBe("");
  });
});
