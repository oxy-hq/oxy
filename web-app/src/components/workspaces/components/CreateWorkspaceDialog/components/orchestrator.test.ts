import { describe, expect, it } from "vitest";
import { deriveRailState, initialState } from "./orchestrator";
import type { OnboardingState } from "./types";

/**
 * The right-rail expected-file list tells the user which artifacts the
 * onboarding builder will create. After migrating the analytics agent from
 * `.agent.yml` (classic) to `.agentic.yml` (multi-step FSM pipeline), the rail
 * must advertise the new file name and type so the download checklist matches
 * what the builder actually writes.
 */
describe("deriveRailState — expected files", () => {
  const baseWithTables: OnboardingState = {
    ...initialState,
    step: "building",
    selectedTables: ["public.orders", "public.customers"]
  };

  it("advertises analytics.agentic.yml as the agent phase output", () => {
    const rail = deriveRailState(baseWithTables);
    const agenticEntry = rail.expectedFiles.find((f) => f.type === "agentic");
    expect(agenticEntry).toEqual({
      name: "analytics.agentic.yml",
      type: "agentic"
    });
  });

  it("no longer advertises a legacy .agent.yml expected file", () => {
    const rail = deriveRailState(baseWithTables);
    expect(rail.expectedFiles.find((f) => f.type === "agent")).toBeUndefined();
    expect(rail.expectedFiles.find((f) => f.name.endsWith(".agent.yml"))).toBeUndefined();
  });

  it("preserves config, per-table view, and app entries around the agentic entry", () => {
    const rail = deriveRailState(baseWithTables);
    const names = rail.expectedFiles.map((f) => f.name);
    // The second-app entry is named after the second topic alphabetically
    // (here: "orders" from ["customers", "orders"]) — not a generic "detail".
    expect(names).toEqual([
      "config.yml",
      "orders",
      "customers",
      "analytics.agentic.yml",
      "overview",
      "orders"
    ]);
  });

  it("omits the second-app entry when the workspace has only one topic", () => {
    const singleTopic: OnboardingState = {
      ...initialState,
      step: "building",
      selectedTables: ["public.orders"]
    };
    const rail = deriveRailState(singleTopic);
    const appEntries = rail.expectedFiles.filter((f) => f.type === "app");
    expect(appEntries.map((f) => f.name)).toEqual(["overview"]);
  });

  it("returns an empty expected-files list until tables are selected", () => {
    const rail = deriveRailState({ ...initialState, step: "building" });
    expect(rail.expectedFiles).toEqual([]);
  });
});
