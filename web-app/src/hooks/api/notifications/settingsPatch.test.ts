import { describe, expect, it } from "vitest";
import {
  oxygen,
  settingsDoc,
  storeOps
} from "@/components/settings/SettingsDialog/sections/account/Notifications/__tests__/fixtures";
import type { NotificationSettings, NotificationSettingsPatch } from "@/types/notifications";
import {
  applySettingsPatch,
  invertSettingsPatch,
  settingsPatchApplies,
  settingsPatchChanges
} from "./settingsPatch";

const doc = () => settingsDoc({ sources: [storeOps(), oxygen()] });
const ops = (d: NotificationSettings) => d.sources[0];
const operations = (d: NotificationSettings) => ops(d).categories[0];
const foodSafety = (d: NotificationSettings) => operations(d).categories[0];
const tempCheck = (d: NotificationSettings) => foodSafety(d).kinds[0];

describe("applySettingsPatch", () => {
  it("sets a channel on a kind, a category and a whole source", () => {
    const after = applySettingsPatch(doc(), {
      nodes: [
        { app_id: "app-1", kind: "temp_check", channels: { push: false } },
        { app_id: "app-1", scope: "operations/food-safety", channels: { slack: false } },
        { app_id: "app-1", scope: "", channels: { email: false } }
      ]
    });
    expect(tempCheck(after).settings.channels).toEqual({ push: false });
    expect(foodSafety(after).settings.channels).toEqual({ email: true, slack: false });
    expect(ops(after).settings.channels).toEqual({ email: false });
  });

  it("takes a channel back to inheriting with null", () => {
    const after = applySettingsPatch(doc(), {
      nodes: [{ app_id: "app-1", scope: "operations", channels: { email: null } }]
    });
    expect(operations(after).settings.channels).toEqual({});
  });

  it("sets and clears a floor", () => {
    const set = applySettingsPatch(doc(), {
      nodes: [{ app_id: "app-1", kind: "temp_check", min_level: "critical" }]
    });
    expect(tempCheck(set).settings).toEqual({ channels: {}, min_level: "critical" });
    const cleared = applySettingsPatch(set, {
      nodes: [{ app_id: "app-1", kind: "temp_check", min_level: null }]
    });
    expect(tempCheck(cleared).settings.min_level).toBeNull();
  });

  it("touches only the source the node names: Oxygen is app_id null", () => {
    const before = doc();
    const after = applySettingsPatch(before, {
      nodes: [{ app_id: null, scope: "runs", channels: { email: false } }]
    });
    expect(after.sources[1].categories[1].settings.channels).toEqual({ email: false });
    expect(after.sources[0]).toEqual(before.sources[0]);
  });

  it("merges the person's settings", () => {
    const after = applySettingsPatch(doc(), { global: { paused_until: "2026-10-18T08:00:00Z" } });
    expect(after.global).toEqual({
      paused_until: "2026-10-18T08:00:00Z",
      quiet_hours: null,
      timezone: null
    });
  });
});

describe("invertSettingsPatch", () => {
  it("undoes exactly what the patch changed", () => {
    const before = doc();
    const patch: NotificationSettingsPatch = {
      global: { timezone: "Europe/Berlin" },
      nodes: [
        { app_id: "app-1", scope: "operations", channels: { email: null, push: false } },
        { app_id: "app-1", kind: "temp_check", min_level: "critical" }
      ]
    };
    const undo = invertSettingsPatch(before, patch);
    expect(undo).toEqual({
      global: { timezone: null },
      nodes: [
        { app_id: "app-1", scope: "operations", channels: { email: false, push: null } },
        { app_id: "app-1", kind: "temp_check", min_level: null }
      ]
    });
    expect(applySettingsPatch(applySettingsPatch(before, patch), undo)).toEqual(before);
  });

  it("leaves another change made since alone", () => {
    const before = doc();
    const mine = { nodes: [{ app_id: "app-1", kind: "temp_check", channels: { push: false } }] };
    const theirs = { nodes: [{ app_id: null, scope: "runs", channels: { email: false } }] };
    const both = applySettingsPatch(applySettingsPatch(before, mine), theirs);
    const undone = applySettingsPatch(both, invertSettingsPatch(before, mine));
    expect(tempCheck(undone).settings.channels).toEqual({});
    expect(undone.sources[1].categories[1].settings.channels).toEqual({ email: false });
  });
});

describe("settingsPatchApplies", () => {
  it("accepts a node that exists and a channel it is sent on", () => {
    expect(
      settingsPatchApplies(doc(), {
        nodes: [{ app_id: "app-1", scope: "operations", channels: { slack: false } }]
      })
    ).toBe(true);
    expect(settingsPatchApplies(doc(), { global: { quiet_hours: null } })).toBe(true);
  });

  it("refuses a node that is gone", () => {
    expect(
      settingsPatchApplies(doc(), {
        nodes: [{ app_id: "app-gone", kind: "temp_check", channels: { push: false } }]
      })
    ).toBe(false);
    expect(
      settingsPatchApplies(doc(), {
        nodes: [{ app_id: "app-1", scope: "operations/gone", channels: { push: false } }]
      })
    ).toBe(false);
  });

  it("refuses a channel the node is never sent on", () => {
    expect(
      settingsPatchApplies(doc(), {
        nodes: [{ app_id: null, kind: "oxy.run_failed", channels: { slack: false } }]
      })
    ).toBe(false);
  });

  it("refuses turning off a node of only critical kinds", () => {
    expect(
      settingsPatchApplies(doc(), {
        nodes: [{ app_id: null, scope: "billing", channels: { push: false } }]
      })
    ).toBe(false);
  });
});

describe("settingsPatchChanges", () => {
  it("is false for a change the document already has", () => {
    expect(
      settingsPatchChanges(doc(), {
        nodes: [{ app_id: "app-1", scope: "operations", channels: { email: false } }]
      })
    ).toBe(false);
    expect(
      settingsPatchChanges(doc(), {
        nodes: [{ app_id: "app-1", scope: "operations", channels: { email: true } }]
      })
    ).toBe(true);
  });
});
