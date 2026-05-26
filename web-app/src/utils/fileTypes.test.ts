import { describe, expect, it } from "vitest";
import { detectFileType, FileType } from "./fileTypes";

describe("detectFileType", () => {
  describe("ANALYTICS_AGENT (.agentic.yml / .agentic.yaml)", () => {
    it("detects .agentic.yml", () => {
      expect(detectFileType("analytics.agentic.yml")).toBe(FileType.ANALYTICS_AGENT);
    });

    it("detects .agentic.yaml", () => {
      expect(detectFileType("training_coach.agentic.yaml")).toBe(FileType.ANALYTICS_AGENT);
    });

    it("detects nested path with .agentic.yml", () => {
      expect(detectFileType("demo_project/analytics.agentic.yml")).toBe(FileType.ANALYTICS_AGENT);
    });
  });

  describe("no regression on existing types", () => {
    it("still detects .workflow.yml as WORKFLOW", () => {
      expect(detectFileType("etl.workflow.yml")).toBe(FileType.WORKFLOW);
    });

    it("still detects .app.yml as APP", () => {
      expect(detectFileType("dashboard.app.yml")).toBe(FileType.APP);
    });

    it("returns DEFAULT for unknown extension", () => {
      expect(detectFileType("README.md")).toBe(FileType.DEFAULT);
    });

    it("returns DEFAULT for retired .agent.yml extension", () => {
      expect(detectFileType("default.agent.yml")).toBe(FileType.DEFAULT);
    });
  });
});
