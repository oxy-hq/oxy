import { describe, expect, it } from "vitest";
import type { FileTreeModel } from "@/types/file";
import { getObjectName, groupObjectsByType } from "./utils";

const makeFile = (name: string, path: string): FileTreeModel => ({
  name,
  path,
  is_dir: false,
  children: []
});

describe("getObjectName", () => {
  it("strips .agentic.yml", () => {
    expect(getObjectName(makeFile("analytics.agentic.yml", "analytics.agentic.yml"))).toBe(
      "analytics"
    );
  });

  it("strips .agentic.yaml", () => {
    expect(
      getObjectName(makeFile("training_coach.agentic.yaml", "training_coach.agentic.yaml"))
    ).toBe("training_coach");
  });

  it("strips .agent.yml (no regression)", () => {
    expect(getObjectName(makeFile("default.agent.yml", "default.agent.yml"))).toBe("default");
  });

  it("strips .workflow.yml (no regression)", () => {
    expect(getObjectName(makeFile("etl.workflow.yml", "etl.workflow.yml"))).toBe("etl");
  });

  it("strips .app.yml (no regression)", () => {
    expect(getObjectName(makeFile("dashboard.app.yml", "dashboard.app.yml"))).toBe("dashboard");
  });
});

describe("groupObjectsByType", () => {
  it("puts .agentic.yml files into the agents group", () => {
    const file = makeFile("analytics.agentic.yml", "analytics.agentic.yml");
    const result = groupObjectsByType([file]);
    expect(result.agents).toContain(file);
    expect(result.procedures).toHaveLength(0);
    expect(result.apps).toHaveLength(0);
  });

  it("puts .agentic.yaml files into the agents group", () => {
    const file = makeFile("coach.agentic.yaml", "coach.agentic.yaml");
    const result = groupObjectsByType([file]);
    expect(result.agents).toContain(file);
  });

  it("does NOT put .agentic.yml files into procedures group", () => {
    const file = makeFile("analytics.agentic.yml", "analytics.agentic.yml");
    const result = groupObjectsByType([file]);
    expect(result.procedures).not.toContain(file);
  });

  it("still groups .agent.yml into agents (no regression)", () => {
    const file = makeFile("default.agent.yml", "default.agent.yml");
    const result = groupObjectsByType([file]);
    expect(result.agents).toContain(file);
  });

  it("still groups .aw.yml into procedures (no regression)", () => {
    const file = makeFile("demo.aw.yml", "demo.aw.yml");
    const result = groupObjectsByType([file]);
    expect(result.procedures).toContain(file);
  });

  it("groups both .agent.yml and .agentic.yml files together in agents", () => {
    const agentFile = makeFile("default.agent.yml", "default.agent.yml");
    const agenticFile = makeFile("analytics.agentic.yml", "analytics.agentic.yml");
    const result = groupObjectsByType([agentFile, agenticFile]);
    expect(result.agents).toContain(agentFile);
    expect(result.agents).toContain(agenticFile);
    expect(result.agents).toHaveLength(2);
  });

  it("ignores directory entries", () => {
    const dir: FileTreeModel = {
      name: "analytics.agentic.yml",
      path: "analytics.agentic.yml",
      is_dir: true,
      children: []
    };
    const result = groupObjectsByType([dir]);
    expect(result.agents).toHaveLength(0);
  });
});
