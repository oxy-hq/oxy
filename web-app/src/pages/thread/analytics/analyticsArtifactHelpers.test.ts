// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";

vi.mock("@lottiefiles/react-lottie-player", () => ({ Player: "div" }));

import type { ArtifactItem } from "@/hooks/analyticsSteps";
import { parseToolJson, sqlArtifactFromExecutePreview } from "./analyticsArtifactHelpers";

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
