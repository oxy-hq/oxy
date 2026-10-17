import { describe, expect, it } from "vitest";
import type { SettingsSource } from "@/types/notifications";
import { category, kind, own, storeOps } from "./__tests__/fixtures";
import { inheritedNote, type PrefRow, prefRows, rowNote } from "./utils";

const rowsByKey = (source: SettingsSource) =>
  Object.fromEntries(prefRows(source).map((r) => [r.key, r])) as Record<string, PrefRow>;

describe("prefRows", () => {
  it("lists the source, then each category followed by its kinds and its own categories", () => {
    expect(prefRows(storeOps()).map((r) => [r.key, r.depth])).toEqual([
      ["source", 0],
      ["cat-operations", 1],
      ["kind-list_overdue", 2],
      ["cat-operations/food-safety", 2],
      ["kind-temp_check", 3],
      ["kind-temp_alarm", 3],
      ["cat-staffing", 1],
      ["kind-shift_swap", 2],
      ["kind-swap_urgent", 2]
    ]);
  });

  it("marks a channel set at the row itself as explicit", () => {
    const rows = rowsByKey(storeOps());
    expect(rows["cat-operations"].state.email).toEqual({ on: false, explicit: true, from: null });
    expect(rows["cat-operations/food-safety"].state.email).toEqual({
      on: true,
      explicit: true,
      from: null
    });
  });

  it("inherits the nearest row above that sets the channel, and names it", () => {
    const rows = rowsByKey(storeOps());
    expect(rows["kind-list_overdue"].state.email).toEqual({
      on: false,
      explicit: false,
      from: "Store operations"
    });
    // Food safety turned email back on under Store operations: the nearer one wins.
    expect(rows["kind-temp_check"].state.email).toEqual({
      on: true,
      explicit: false,
      from: "Food safety"
    });
  });

  it("is on, from nowhere, when no row sets the channel", () => {
    const rows = rowsByKey(storeOps());
    expect(rows["kind-list_overdue"].state.push).toEqual({ on: true, explicit: false, from: null });
    expect(rows.source.state.push).toEqual({ on: true, explicit: false, from: null });
  });

  it("puts a kind below the nearest floor in the inbox only, naming the row that set it", () => {
    const rows = rowsByKey(storeOps());
    expect(rows["kind-shift_swap"].inboxOnly).toEqual({ from: "Staffing" });
    expect(rows["kind-swap_urgent"].inboxOnly).toBeNull();
  });

  it("lets a kind's own floor beat the one above it", () => {
    const source = storeOps();
    source.categories[1].kinds[0].settings = own({ min_level: "passive" });
    expect(rowsByKey(source)["kind-shift_swap"].inboxOnly).toBeNull();
    source.categories[1].kinds[0].settings = own({ min_level: "critical" });
    expect(rowsByKey(source)["kind-shift_swap"].inboxOnly).toEqual({ from: null });
  });

  it("never holds a critical kind back, and locks a subtree of only critical kinds", () => {
    const source: SettingsSource = {
      ...storeOps(),
      settings: own({ min_level: "critical", channels: { push: false } }),
      categories: [
        category({
          path: "alarms",
          label: "Alarms",
          kinds: [kind({ kind: "fire", label: "Fire", level: "critical", critical: true })]
        })
      ]
    };
    const rows = rowsByKey(source);
    expect(rows["kind-fire"].inboxOnly).toBeNull();
    expect(rows["kind-fire"].critical).toBe(true);
    expect(rows["cat-alarms"].critical).toBe(true);
    expect(rows.source.critical).toBe(true);
  });

  it("draws only the channels something under the row is sent on", () => {
    const source: SettingsSource = {
      ...storeOps(),
      categories: [
        category({
          path: "a",
          label: "A",
          kinds: [kind({ kind: "k", label: "K", channels: ["push"] })]
        })
      ]
    };
    const rows = rowsByKey(source);
    expect(rows["cat-a"].channels).toEqual(["push"]);
    expect(rows["cat-a"].state.email).toBeUndefined();
  });
});

describe("rowNote", () => {
  it("says a critical row is always delivered", () => {
    expect(rowNote(rowsByKey(storeOps())["kind-temp_alarm"])).toBe("Critical — always delivered");
  });

  it("says a held-back kind is inbox only, and where the floor is", () => {
    expect(rowNote(rowsByKey(storeOps())["kind-shift_swap"])).toBe(
      "Inbox only — floor on Staffing"
    );
  });

  it("shows a floor set at the row", () => {
    expect(rowNote(rowsByKey(storeOps())["cat-staffing"])).toBe("Time-sensitive and above");
  });

  it("names only the channels inherited from a row, when others are not", () => {
    expect(rowNote(rowsByKey(storeOps())["kind-list_overdue"])).toBe("Email from Store operations");
  });

  it("says just 'From <row>' when every inherited switch comes from it", () => {
    const source = storeOps();
    source.settings = own({ channels: { push: false, email: false, slack: true } });
    const rows = rowsByKey(source);
    expect(inheritedNote(rows["cat-staffing"])).toBe("From Store Ops");
    // Store operations sets its own email; push and Slack still come from Store Ops.
    expect(inheritedNote(rows["cat-operations"])).toBe("From Store Ops");
    expect(inheritedNote(rows["kind-list_overdue"])).toBe(
      "Push, Slack from Store Ops · Email from Store operations"
    );
  });

  it("says nothing on a row that inherits only the defaults", () => {
    expect(rowNote(rowsByKey(storeOps()).source)).toBeNull();
  });
});
