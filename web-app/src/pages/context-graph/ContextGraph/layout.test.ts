import { describe, expect, it } from "vitest";
import type { ContextGraphNode } from "@/types/contextGraph";
import { buildInitialEdges, buildInitialNodes, layoutRow } from "./layout";

const makeNode = (id: string, type: ContextGraphNode["type"], label: string): ContextGraphNode => ({
  id,
  type,
  label,
  data: { name: id }
});

describe("layoutRow", () => {
  it("positions nodes with correct x/y offsets", () => {
    const row = [
      { node: makeNode("a", "agent", "A"), width: 150 },
      { node: makeNode("b", "agent", "B"), width: 150 }
    ];
    const result = layoutRow(row, 0);
    expect(result).toHaveLength(2);
    expect(result[0].position.y).toBe(0);
    expect(result[1].position.y).toBe(0);
    // Second node x = first x + first width + PADDING(40)
    expect(result[1].position.x).toBe(result[0].position.x + 150 + 40);
  });

  it("centers nodes horizontally (negative x start)", () => {
    const row = [{ node: makeNode("a", "agent", "A"), width: 200 }];
    const result = layoutRow(row, 0);
    // totalWidth = 200, x starts at -200/2 = -100
    expect(result[0].position.x).toBe(-100);
  });

  it("uses rowIndex * ROW_HEIGHT for y position", () => {
    const row = [{ node: makeNode("a", "agent", "A"), width: 150 }];
    const result = layoutRow(row, 3);
    expect(result[0].position.y).toBe(3 * 80);
  });

  it("sets node type to 'context-graph' and zIndex to 10", () => {
    const row = [{ node: makeNode("a", "agent", "A"), width: 150 }];
    const result = layoutRow(row, 0);
    expect(result[0].type).toBe("context-graph");
    expect(result[0].zIndex).toBe(10);
  });
});

describe("buildInitialNodes", () => {
  it("groups by type in TYPE_ORDER", () => {
    const nodes = [makeNode("t1", "table", "Table"), makeNode("a1", "agent", "Agent")];
    const result = buildInitialNodes(nodes);
    const agentIdx = result.findIndex((n) => n.id === "a1");
    const tableIdx = result.findIndex((n) => n.id === "t1");
    expect(agentIdx).toBeLessThan(tableIdx);
  });

  it("overflows into multiple rows when exceeding MAX_ROW_WIDTH", () => {
    const nodes = Array.from({ length: 15 }, (_, i) =>
      makeNode(`a${i}`, "agent", `Agent With A Long Name ${i}`)
    );
    const result = buildInitialNodes(nodes);
    const yValues = new Set(result.map((n) => n.position.y));
    expect(yValues.size).toBeGreaterThan(1);
  });

  it("skips types with no nodes", () => {
    const nodes = [makeNode("a1", "agent", "Agent")];
    const result = buildInitialNodes(nodes);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe("a1");
  });
});

describe("buildInitialEdges", () => {
  it("maps edges with correct default styling", () => {
    const edges = [
      { id: "e1", source: "a", target: "b" },
      { id: "e2", source: "c", target: "d" }
    ];
    const result = buildInitialEdges(edges);
    expect(result).toHaveLength(2);
    expect(result[0]).toMatchObject({
      id: "e1",
      source: "a",
      target: "b",
      type: "default",
      style: { stroke: "var(--muted-foreground)", strokeWidth: 1, opacity: 0.15 },
      zIndex: 0
    });
  });
});
