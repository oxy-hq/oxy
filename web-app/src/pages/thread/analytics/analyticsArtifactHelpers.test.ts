// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";

vi.mock("@lottiefiles/react-lottie-player", () => ({ Player: "div" }));

import type { ArtifactItem } from "@/hooks/analyticsSteps";
import type { AnalyticsDisplayBlock } from "@/hooks/useAnalyticsRun";
import {
  parseToolJson,
  sqlArtifactFromExecutePreview,
  toDisplayProps
} from "./analyticsArtifactHelpers";

// ── parseToolJson ───────────────────────────────────────────────────────────

describe("parseToolJson", () => {
  it("parses a double-encoded JSON string", () => {
    const raw = JSON.stringify(JSON.stringify({ a: 1 }));
    expect(parseToolJson(raw)).toEqual({ a: 1 });
  });

  it("returns null for undefined input", () => {
    expect(parseToolJson(undefined)).toBeNull();
  });
});

// ── sqlArtifactFromExecutePreview ───────────────────────────────────────────

function makeArtifact(
  input: Record<string, unknown>,
  output: Record<string, unknown> | undefined
): ArtifactItem {
  return {
    kind: "artifact",
    id: "test-id",
    toolName: "execute_preview",
    toolInput: JSON.stringify(JSON.stringify(input)),
    toolOutput: output ? JSON.stringify(JSON.stringify(output)) : undefined,
    isStreaming: false
  };
}

describe("sqlArtifactFromExecutePreview", () => {
  it("handles rows that are arrays", () => {
    const item = makeArtifact(
      { sql: "SELECT 1" },
      {
        columns: ["a", "b"],
        rows: [
          ["1", "2"],
          ["3", "4"]
        ]
      }
    );
    const result = sqlArtifactFromExecutePreview(item);
    expect(result?.content.value.result).toEqual([
      ["a", "b"],
      ["1", "2"],
      ["3", "4"]
    ]);
  });

  it("handles rows that are objects (not arrays)", () => {
    const item = makeArtifact(
      { sql: "SELECT 1" },
      {
        columns: ["a", "b"],
        rows: [
          { a: "1", b: "2" },
          { a: "3", b: "4" }
        ]
      }
    );
    const result = sqlArtifactFromExecutePreview(item);
    expect(result?.content.value.result).toEqual([
      ["a", "b"],
      ["1", "2"],
      ["3", "4"]
    ]);
  });

  it("handles rows with null/undefined values", () => {
    const item = makeArtifact(
      { sql: "SELECT 1" },
      { columns: ["a", "b"], rows: [[null, undefined]] }
    );
    const result = sqlArtifactFromExecutePreview(item);
    expect(result?.content.value.result).toEqual([
      ["a", "b"],
      ["", ""]
    ]);
  });

  it("returns null when sql is missing", () => {
    const item = makeArtifact({}, { columns: ["a"], rows: [] });
    expect(sqlArtifactFromExecutePreview(item)).toBeNull();
  });
});

// ── toDisplayProps — unique data keys per block ─────────────────────────────

describe("toDisplayProps", () => {
  const makeBlock = (
    chartType: string,
    columns: string[],
    rows: unknown[][],
    title?: string
  ): AnalyticsDisplayBlock => ({
    config: { chart_type: chartType, title } as AnalyticsDisplayBlock["config"],
    columns,
    rows
  });

  it("uses unique data keys for different block indices within the same run", () => {
    const block0 = makeBlock("line_chart", ["week", "value"], [["2024-01", 10]]);
    const block1 = makeBlock("bar_chart", ["month", "count"], [["Jan", 5]]);

    const props0 = toDisplayProps(block0, 0, "run-A");
    const props1 = toDisplayProps(block1, 1, "run-A");

    const dataKey0 = props0.display.data;
    const dataKey1 = props1.display.data;
    expect(dataKey0).not.toBe(dataKey1);

    expect(Object.keys(props0.data)).toEqual([dataKey0]);
    expect(Object.keys(props1.data)).toEqual([dataKey1]);
    expect(props0.data[dataKey0].file_path).not.toBe(props1.data[dataKey1].file_path);
  });

  it("uses unique data keys for the same block index across different runs", () => {
    const block = makeBlock("line_chart", ["week", "value"], [["2024-01", 10]]);

    const propsRunA = toDisplayProps(block, 0, "run-A");
    const propsRunB = toDisplayProps(block, 0, "run-B");

    const dataKeyA = propsRunA.display.data;
    const dataKeyB = propsRunB.display.data;
    expect(dataKeyA).not.toBe(dataKeyB);
    expect(propsRunA.data[dataKeyA].file_path).not.toBe(propsRunB.data[dataKeyB].file_path);
  });

  it("embeds correct JSON in each block's data", () => {
    const block = makeBlock(
      "line_chart",
      ["x", "y"],
      [
        ["a", 1],
        ["b", 2]
      ],
      "My Chart"
    );
    const { data, display } = toDisplayProps(block, 0, "run-1");
    const key = display.data;
    const json = JSON.parse(data[key].json!);
    expect(json).toEqual([
      { x: "a", y: 1 },
      { x: "b", y: 2 }
    ]);
  });
});
