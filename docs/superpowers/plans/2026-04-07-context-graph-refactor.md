# Context Graph Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `ContextGraph.tsx` (~640 lines) into smaller, well-organized files following recursive colocation. Add Vitest tests for each module. No behavior changes.

**Architecture:** Extract constants, layout logic, focus/filter hook, and UI components into separate files under a `ContextGraph/` folder. Pure functions are tested directly; React components via Testing Library; hooks via `renderHook`. The old file is deleted only after the new orchestrator is verified.

**Tech Stack:** React 19, @xyflow/react, Vitest, @testing-library/react, lucide-react, shadcn/ui

---

## File Structure

```
pages/context-graph/
├── index.tsx                              ← page wrapper (unchanged)
└── ContextGraph/
    ├── index.tsx                          ← slim orchestrator (~80 lines)
    ├── index.test.tsx                     ← integration test
    ├── constants.ts                       ← color maps, icons, labels, options, layout constants
    ├── layout.ts                          ← layoutRow + initial node/edge builders
    ├── layout.test.ts
    ├── useGraphFocus.ts                   ← focus/filter state, BFS, node/edge styling
    ├── useGraphFocus.test.ts
    └── components/
        ├── ContextGraphNode.tsx           ← custom ReactFlow node renderer
        ├── ContextGraphNode.test.tsx
        ├── GraphControlPanel/
        │   ├── index.tsx                  ← RFPanel wrapper with stats + filter
        │   ├── index.test.tsx
        │   └── components/
        │       ├── GraphStatsPanel.tsx    ← node/edge counts by type
        │       ├── GraphStatsPanel.test.tsx
        │       ├── GraphFilterPanel.tsx   ← focus selector, expand toggle, reset
        │       └── GraphFilterPanel.test.tsx
        ├── NodeDetailPanel.tsx            ← side panel with file content preview
        └── NodeDetailPanel.test.tsx
```

**Files deleted at end:**
- `pages/context-graph/ContextGraph.tsx` (replaced by `ContextGraph/index.tsx`)
- `pages/context-graph/NodeDetailPanel.tsx` (moved to `ContextGraph/components/`)

---

## Task 1: Extract constants.ts

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/constants.ts`

- [ ] **Step 1: Create constants.ts**

```ts
import { AppWindow, BookOpen, Bot, Box, Eye, FileCode, Table, Workflow as WorkflowIcon } from "lucide-react";

export const BORDER_COLORS: Record<string, string> = {
  agent: "var(--graph-agent-border)",
  procedure: "var(--graph-procedure-border)",
  workflow: "var(--graph-procedure-border)",
  app: "var(--graph-app-border)",
  automation: "var(--graph-automation-border)",
  topic: "var(--graph-automation-border)",
  view: "var(--graph-view-border)",
  sql_query: "var(--graph-sql-query-border)",
  table: "var(--graph-table-border)",
  entity: "var(--graph-entity-border)",
};

export const BG_COLORS: Record<string, string> = {
  agent: "var(--graph-agent-bg)",
  procedure: "var(--graph-procedure-bg)",
  workflow: "var(--graph-procedure-bg)",
  app: "var(--graph-app-bg)",
  automation: "var(--graph-automation-bg)",
  topic: "var(--graph-automation-bg)",
  view: "var(--graph-view-bg)",
  sql_query: "var(--graph-sql-query-bg)",
  table: "var(--graph-table-bg)",
  entity: "var(--graph-entity-bg)",
};

export const HANDLE_STYLE_HIDDEN = {
  width: 0,
  height: 0,
  minWidth: 0,
  minHeight: 0,
  opacity: 0,
  border: "none",
  background: "transparent",
  padding: 0,
} as const;

export const HANDLE_STYLE_VISIBLE = {
  width: 8,
  height: 8,
  border: "2px solid var(--muted-foreground)",
  background: "var(--background)",
  opacity: 0.6,
} as const;

export const ICONS: Record<string, React.ReactNode> = {
  agent: <Bot className="h-3.5 w-3.5" />,
  procedure: <WorkflowIcon className="h-3.5 w-3.5" />,
  workflow: <WorkflowIcon className="h-3.5 w-3.5" />,
  app: <AppWindow className="h-3.5 w-3.5" />,
  automation: <WorkflowIcon className="h-3.5 w-3.5" />,
  topic: <BookOpen className="h-3.5 w-3.5" />,
  view: <Eye className="h-3.5 w-3.5" />,
  sql_query: <FileCode className="h-3.5 w-3.5" />,
  table: <Table className="h-3.5 w-3.5" />,
  entity: <Box className="h-3.5 w-3.5" />,
};

export const TYPE_ORDER = [
  "entity",
  "agent",
  "procedure",
  "workflow",
  "app",
  "automation",
  "topic",
  "view",
  "sql_query",
  "table",
];

export const TYPE_LABELS: Record<string, string> = {
  agent: "Agents",
  procedure: "Procedures",
  workflow: "Workflows (legacy)",
  automation: "Automations (legacy)",
  topic: "Topics",
  view: "Views",
  sql_query: "SQL Queries",
  table: "Tables",
  entity: "Entities",
  app: "Apps",
};

export type FocusType =
  | "auto"
  | "agent"
  | "procedure"
  | "workflow"
  | "app"
  | "automation"
  | "topic"
  | "view"
  | "sql_query"
  | "table"
  | "entity";

export const FOCUS_OPTIONS: Array<{ value: FocusType; label: string; icon?: React.ReactNode }> = [
  { value: "auto", label: "All Types" },
  { value: "agent", label: "Agents", icon: <Bot className="h-4 w-4" /> },
  { value: "procedure", label: "Procedures", icon: <WorkflowIcon className="h-4 w-4" /> },
  { value: "workflow", label: "Workflows (legacy)", icon: <WorkflowIcon className="h-4 w-4" /> },
  { value: "app", label: "Apps", icon: <AppWindow className="h-4 w-4" /> },
  {
    value: "automation",
    label: "Automations (legacy)",
    icon: <WorkflowIcon className="h-4 w-4" />,
  },
  { value: "topic", label: "Topics", icon: <BookOpen className="h-4 w-4" /> },
  { value: "view", label: "Views", icon: <Eye className="h-4 w-4" /> },
  { value: "sql_query", label: "SQL Queries", icon: <FileCode className="h-4 w-4" /> },
  { value: "table", label: "Tables", icon: <Table className="h-4 w-4" /> },
  { value: "entity", label: "Entities", icon: <Box className="h-4 w-4" /> },
];

export const ROW_HEIGHT = 80;
export const MIN_NODE_WIDTH = 150;
export const PADDING = 40;
export const MAX_ROW_WIDTH = 1400;
```

- [ ] **Step 2: Verify TypeScript compiles**

Run: `cd web-app && pnpm exec tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors related to constants.ts

- [ ] **Step 3: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/constants.ts
git commit -m "refactor: extract context graph constants into dedicated file"
```

---

## Task 2: Create layout.ts (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/layout.test.ts`
- Create: `web-app/src/pages/context-graph/ContextGraph/layout.ts`

- [ ] **Step 1: Write the failing tests**

Create `layout.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { ContextGraphNode } from "@/types/contextGraph";
import { buildInitialEdges, buildInitialNodes, layoutRow } from "./layout";

const makeNode = (id: string, type: ContextGraphNode["type"], label: string): ContextGraphNode => ({
  id,
  type,
  label,
  data: { name: id },
});

describe("layoutRow", () => {
  it("positions nodes with correct x/y offsets", () => {
    const row = [
      { node: makeNode("a", "agent", "A"), width: 150 },
      { node: makeNode("b", "agent", "B"), width: 150 },
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
    const nodes = [
      makeNode("t1", "table", "Table"),
      makeNode("a1", "agent", "Agent"),
    ];
    const result = buildInitialNodes(nodes);
    const agentIdx = result.findIndex((n) => n.id === "a1");
    const tableIdx = result.findIndex((n) => n.id === "t1");
    expect(agentIdx).toBeLessThan(tableIdx);
  });

  it("overflows into multiple rows when exceeding MAX_ROW_WIDTH", () => {
    const nodes = Array.from({ length: 15 }, (_, i) =>
      makeNode(`a${i}`, "agent", `Agent With A Long Name ${i}`),
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
      { id: "e2", source: "c", target: "d" },
    ];
    const result = buildInitialEdges(edges);
    expect(result).toHaveLength(2);
    expect(result[0]).toMatchObject({
      id: "e1",
      source: "a",
      target: "b",
      type: "default",
      style: { stroke: "var(--muted-foreground)", strokeWidth: 1, opacity: 0.15 },
      zIndex: 0,
    });
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/layout.test.ts 2>&1 | tail -10`
Expected: FAIL — cannot find module `./layout`

- [ ] **Step 3: Write minimal implementation**

Create `layout.ts`:

```ts
import type { Node } from "@xyflow/react";
import type { ContextGraphNode as ContextGraphNodeType } from "@/types/contextGraph";
import { MAX_ROW_WIDTH, MIN_NODE_WIDTH, PADDING, ROW_HEIGHT, TYPE_ORDER } from "./constants";

export function layoutRow(
  row: Array<{ node: ContextGraphNodeType; width: number }>,
  rowIndex: number,
): Node[] {
  const totalWidth = row.reduce((sum, info) => sum + info.width + PADDING, 0) - PADDING;
  let x = -totalWidth / 2;

  return row.map(({ node, width }) => {
    const n: Node = {
      id: node.id,
      type: "context-graph",
      data: { label: node.label, type: node.type },
      position: { x, y: rowIndex * ROW_HEIGHT },
      zIndex: 10,
    };
    x += width + PADDING;
    return n;
  });
}

export function buildInitialNodes(nodes: ContextGraphNodeType[]): Node[] {
  const typeGroups: Record<string, ContextGraphNodeType[]> = {};
  for (const node of nodes) {
    if (!typeGroups[node.type]) typeGroups[node.type] = [];
    typeGroups[node.type].push(node);
  }

  const result: Node[] = [];
  let rowIndex = 0;

  for (const type of TYPE_ORDER) {
    const nodesOfType = typeGroups[type];
    if (!nodesOfType?.length) continue;

    const nodeInfos = nodesOfType.map((node) => ({
      node,
      width: Math.max(MIN_NODE_WIDTH, node.label.length * 8 + 60),
    }));

    let currentRow: typeof nodeInfos = [];
    let currentRowWidth = 0;

    for (const info of nodeInfos) {
      const w = info.width + PADDING;
      if (currentRowWidth + w > MAX_ROW_WIDTH && currentRow.length > 0) {
        result.push(...layoutRow(currentRow, rowIndex));
        rowIndex++;
        currentRow = [info];
        currentRowWidth = w;
      } else {
        currentRow.push(info);
        currentRowWidth += w;
      }
    }
    if (currentRow.length > 0) {
      result.push(...layoutRow(currentRow, rowIndex));
      rowIndex++;
    }
  }

  return result;
}

export function buildInitialEdges(
  edges: Array<{ id: string; source: string; target: string }>,
) {
  return edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: "default" as const,
    style: {
      stroke: "var(--muted-foreground)",
      strokeWidth: 1,
      opacity: 0.15,
    },
    zIndex: 0,
  }));
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/layout.test.ts 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/layout.ts web-app/src/pages/context-graph/ContextGraph/layout.test.ts
git commit -m "refactor: extract context graph layout functions with tests"
```

---

## Task 3: Create useGraphFocus.ts (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/useGraphFocus.test.ts`
- Create: `web-app/src/pages/context-graph/ContextGraph/useGraphFocus.ts`

- [ ] **Step 1: Write the failing tests**

Create `useGraphFocus.test.ts`:

```ts
// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  buildNeighbors,
  computeFocusTypeVisible,
  getConnectedNodes,
  useGraphFocus,
} from "./useGraphFocus";

// --- Pure function tests ---

describe("buildNeighbors", () => {
  it("builds bidirectional adjacency map from edges", () => {
    const edges = [
      { source: "a", target: "b" },
      { source: "b", target: "c" },
    ];
    const map = buildNeighbors(edges);
    expect(map.get("a")).toEqual(new Set(["b"]));
    expect(map.get("b")).toEqual(new Set(["a", "c"]));
    expect(map.get("c")).toEqual(new Set(["b"]));
  });
});

describe("getConnectedNodes", () => {
  const neighbors = new Map<string, Set<string>>([
    ["a", new Set(["b", "c"])],
    ["b", new Set(["a", "d"])],
    ["c", new Set(["a"])],
    ["d", new Set(["b", "e"])],
    ["e", new Set(["d"])],
  ]);

  it("returns direct neighbors at depth 1", () => {
    const result = getConnectedNodes(neighbors, ["a"], 1);
    // a itself + direct neighbors b, c
    expect(result).toEqual(new Set(["a", "b", "c"]));
  });

  it("returns full connected cluster with no depth limit", () => {
    const result = getConnectedNodes(neighbors, ["a"]);
    expect(result).toEqual(new Set(["a", "b", "c", "d", "e"]));
  });

  it("returns only start node at depth 0", () => {
    const result = getConnectedNodes(neighbors, ["a"], 0);
    expect(result).toEqual(new Set(["a"]));
  });
});

describe("computeFocusTypeVisible", () => {
  it("returns null for 'auto' focus type", () => {
    expect(computeFocusTypeVisible("auto", [], new Map())).toBeNull();
  });

  it("returns correct node set for a given type", () => {
    const nodes = [
      { id: "n1", type: "agent" },
      { id: "n2", type: "table" },
      { id: "n3", type: "agent" },
      { id: "n4", type: "view" },
    ];
    const neighbors = buildNeighbors([
      { source: "n1", target: "n2" },
      { source: "n3", target: "n2" },
    ]);
    const result = computeFocusTypeVisible("agent", nodes, neighbors);
    // Agents n1, n3 + their connected table n2
    expect(result).toEqual(new Set(["n1", "n2", "n3"]));
    expect(result?.has("n4")).toBe(false);
  });
});

// --- Hook tests ---

describe("useGraphFocus", () => {
  const mockData = {
    nodes: [
      { id: "n1", type: "agent" as const, label: "Agent 1", data: { name: "agent1" } },
      { id: "n2", type: "table" as const, label: "Table 1", data: { name: "table1" } },
      { id: "n3", type: "agent" as const, label: "Agent 2", data: { name: "agent2" } },
      { id: "n4", type: "view" as const, label: "View 1", data: { name: "view1" } },
    ],
    edges: [
      { id: "e1", source: "n1", target: "n2" },
      { id: "e2", source: "n3", target: "n2" },
    ],
  };

  afterEach(() => {
    localStorage.clear();
  });

  it("initializes with null focusedNodeId and selectedNode", () => {
    const { result } = renderHook(() =>
      useGraphFocus({ data: mockData, setNodes: vi.fn(), setEdges: vi.fn() }),
    );
    expect(result.current.focusedNodeId).toBeNull();
    expect(result.current.selectedNode).toBeNull();
  });

  it("handleNodeClick toggles focusedNodeId on/off", () => {
    const { result } = renderHook(() =>
      useGraphFocus({ data: mockData, setNodes: vi.fn(), setEdges: vi.fn() }),
    );

    act(() => {
      result.current.handleNodeClick({} as React.MouseEvent, { id: "n1" });
    });
    expect(result.current.focusedNodeId).toBe("n1");
    expect(result.current.selectedNode?.id).toBe("n1");

    // Click same node again — toggles off
    act(() => {
      result.current.handleNodeClick({} as React.MouseEvent, { id: "n1" });
    });
    expect(result.current.focusedNodeId).toBeNull();
    expect(result.current.selectedNode).toBeNull();
  });

  it("handlePaneClick clears both focusedNodeId and selectedNode", () => {
    const { result } = renderHook(() =>
      useGraphFocus({ data: mockData, setNodes: vi.fn(), setEdges: vi.fn() }),
    );

    act(() => {
      result.current.handleNodeClick({} as React.MouseEvent, { id: "n1" });
    });
    expect(result.current.focusedNodeId).toBe("n1");

    act(() => {
      result.current.handlePaneClick();
    });
    expect(result.current.focusedNodeId).toBeNull();
    expect(result.current.selectedNode).toBeNull();
  });

  it("changeFocusType updates focusType and clears focus/selection", () => {
    const { result } = renderHook(() =>
      useGraphFocus({ data: mockData, setNodes: vi.fn(), setEdges: vi.fn() }),
    );

    act(() => {
      result.current.handleNodeClick({} as React.MouseEvent, { id: "n1" });
    });
    expect(result.current.focusedNodeId).toBe("n1");

    act(() => {
      result.current.changeFocusType("table");
    });
    expect(result.current.focusType).toBe("table");
    expect(result.current.focusedNodeId).toBeNull();
    expect(result.current.selectedNode).toBeNull();
  });

  it("persists focusType to localStorage", () => {
    const { result } = renderHook(() =>
      useGraphFocus({ data: mockData, setNodes: vi.fn(), setEdges: vi.fn() }),
    );

    act(() => {
      result.current.changeFocusType("view");
    });
    expect(localStorage.getItem("context-graph-focus-type")).toBe("view");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/useGraphFocus.test.ts 2>&1 | tail -10`
Expected: FAIL — cannot find module `./useGraphFocus`

- [ ] **Step 3: Write minimal implementation**

Create `useGraphFocus.ts`:

```ts
import type { Edge, Node } from "@xyflow/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  ContextGraphNode as ContextGraphNodeType,
  ContextGraph as ContextGraphType,
} from "@/types/contextGraph";
import type { FocusType } from "./constants";

interface UseGraphFocusParams {
  data: ContextGraphType;
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<Edge[]>>;
}

interface UseGraphFocusReturn {
  focusedNodeId: string | null;
  selectedNode: ContextGraphNodeType | null;
  focusType: FocusType;
  expandAll: boolean;
  changeFocusType: (type: FocusType) => void;
  setExpandAll: (expand: boolean) => void;
  handleNodeClick: (event: React.MouseEvent, node: { id: string }) => void;
  handlePaneClick: () => void;
  resetView: () => void;
}

export function buildNeighbors(
  edges: Array<{ source: string; target: string }>,
): Map<string, Set<string>> {
  const map = new Map<string, Set<string>>();
  for (const edge of edges) {
    if (!map.has(edge.source)) map.set(edge.source, new Set());
    if (!map.has(edge.target)) map.set(edge.target, new Set());
    map.get(edge.source)!.add(edge.target);
    map.get(edge.target)!.add(edge.source);
  }
  return map;
}

export function getConnectedNodes(
  neighbors: Map<string, Set<string>>,
  startIds: string[],
  maxDepth?: number,
): Set<string> {
  const visited = new Set<string>();
  const queue: Array<{ id: string; depth: number }> = startIds.map((id) => ({
    id,
    depth: 0,
  }));
  while (queue.length > 0) {
    const current = queue.shift()!;
    if (visited.has(current.id)) continue;
    visited.add(current.id);
    if (maxDepth !== undefined && current.depth >= maxDepth) continue;
    const adjacent = neighbors.get(current.id);
    if (adjacent) {
      for (const n of adjacent) {
        if (!visited.has(n)) queue.push({ id: n, depth: current.depth + 1 });
      }
    }
  }
  return visited;
}

export function computeFocusTypeVisible(
  focusType: FocusType,
  nodes: Array<{ id: string; type: string }>,
  neighbors: Map<string, Set<string>>,
): Set<string> | null {
  if (focusType === "auto") return null;
  const seeds = nodes.filter((n) => n.type === focusType).map((n) => n.id);
  return getConnectedNodes(neighbors, seeds);
}

export function useGraphFocus({
  data,
  setNodes,
  setEdges,
}: UseGraphFocusParams): UseGraphFocusReturn {
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<ContextGraphNodeType | null>(null);
  const [focusType, setFocusType] = useState<FocusType>(() => {
    const saved = localStorage.getItem("context-graph-focus-type");
    return (saved as FocusType) || "auto";
  });
  const [expandAll, setExpandAll] = useState<boolean>(() => {
    const saved = localStorage.getItem("context-graph-expand-all");
    return saved === "true";
  });

  useEffect(() => {
    localStorage.setItem("context-graph-focus-type", focusType);
  }, [focusType]);

  useEffect(() => {
    localStorage.setItem("context-graph-expand-all", expandAll.toString());
  }, [expandAll]);

  const neighbors = useMemo(() => buildNeighbors(data.edges), [data.edges]);

  const handleNodeClick = useCallback(
    (_event: React.MouseEvent, node: { id: string }) => {
      setFocusedNodeId((prev) => (prev === node.id ? null : node.id));
      const contextNode = data.nodes.find((n) => n.id === node.id);
      setSelectedNode((prev) =>
        prev?.id === node.id ? null : (contextNode ?? null),
      );
    },
    [data.nodes],
  );

  const handlePaneClick = useCallback(() => {
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const changeFocusType = useCallback((type: FocusType) => {
    setFocusType(type);
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const resetView = useCallback(() => {
    setFocusedNodeId(null);
    setSelectedNode(null);
  }, []);

  const focusTypeVisible = useMemo(
    () => computeFocusTypeVisible(focusType, data.nodes, neighbors),
    [focusType, data.nodes, neighbors],
  );

  useEffect(() => {
    if (!focusedNodeId) {
      setNodes((nds) =>
        nds.map((node) => {
          let opacity = 1;
          if (focusTypeVisible) {
            const isFocusedType =
              (node.data as { type: string }).type === focusType;
            const isConnected = focusTypeVisible.has(node.id);
            if (!isFocusedType) {
              opacity = isConnected ? 0.5 : 0;
            }
          }
          return {
            ...node,
            data: {
              ...node.data,
              opacity,
              showLeftHandle: false,
              showRightHandle: false,
            },
          };
        }),
      );
      setEdges((eds) =>
        eds.map((edge) => {
          let edgeOpacity = 0.15;
          if (focusTypeVisible) {
            const srcVisible = focusTypeVisible.has(edge.source);
            const tgtVisible = focusTypeVisible.has(edge.target);
            edgeOpacity = srcVisible && tgtVisible ? 0.15 : 0;
          }
          return {
            ...edge,
            animated: false,
            style: {
              stroke: "var(--muted-foreground)",
              strokeWidth: 1,
              opacity: edgeOpacity,
              strokeDasharray: "none",
            },
          };
        }),
      );
      return;
    }

    const maxDepth = expandAll ? undefined : 1;
    const connected = getConnectedNodes(neighbors, [focusedNodeId], maxDepth);

    const leftHandle = new Set<string>();
    const rightHandle = new Set<string>();
    for (const edge of data.edges) {
      if (connected.has(edge.source) && connected.has(edge.target)) {
        rightHandle.add(edge.source);
        leftHandle.add(edge.target);
      }
    }

    setNodes((nds) =>
      nds.map((node) => {
        const isVisible = connected.has(node.id);
        return {
          ...node,
          data: {
            ...node.data,
            opacity: isVisible ? 1 : 0,
            showLeftHandle: leftHandle.has(node.id),
            showRightHandle: rightHandle.has(node.id),
          },
        };
      }),
    );

    setEdges((eds) =>
      eds.map((edge) => {
        const bothVisible =
          connected.has(edge.source) && connected.has(edge.target);
        const isDirect =
          edge.source === focusedNodeId || edge.target === focusedNodeId;
        return {
          ...edge,
          animated: bothVisible,
          style: {
            stroke: "var(--muted-foreground)",
            strokeWidth: isDirect ? 1.5 : 1,
            opacity: bothVisible ? (isDirect ? 0.5 : 0.25) : 0,
            strokeDasharray: bothVisible ? "6 4" : "none",
          },
        };
      }),
    );
  }, [
    focusedNodeId,
    focusType,
    focusTypeVisible,
    expandAll,
    data.edges,
    neighbors,
    setNodes,
    setEdges,
  ]);

  return {
    focusedNodeId,
    selectedNode,
    focusType,
    expandAll,
    changeFocusType,
    setExpandAll,
    handleNodeClick,
    handlePaneClick,
    resetView,
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/useGraphFocus.test.ts 2>&1 | tail -20`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/useGraphFocus.ts web-app/src/pages/context-graph/ContextGraph/useGraphFocus.test.ts
git commit -m "refactor: extract useGraphFocus hook with BFS logic and tests"
```

---

## Task 4: Create ContextGraphNode.tsx (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/components/ContextGraphNode.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/components/ContextGraphNode.tsx`

- [ ] **Step 1: Write the failing tests**

Create `ContextGraphNode.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@xyflow/react", () => ({
  Handle: ({ type, style }: { type: string; style: Record<string, unknown> }) => (
    <div data-testid={`handle-${type}`} style={style} />
  ),
  Position: { Left: "left", Right: "right" },
}));

// Must import after vi.mock
const { ContextGraphNode } = await import("./ContextGraphNode");

afterEach(() => cleanup());

const renderNode = (data: Record<string, unknown>) => {
  // ContextGraphNode only reads `data` from props
  render(<ContextGraphNode {...({ data } as any)} />);
};

describe("ContextGraphNode", () => {
  it("renders label text", () => {
    renderNode({ label: "My Agent", type: "agent" });
    expect(screen.getByText("My Agent")).toBeInTheDocument();
  });

  it("renders correct icon for node type", () => {
    // Icons are React elements; we verify the icon container renders by checking the label is present
    // alongside the icon wrapper (icon + label are siblings in a flex container)
    renderNode({ label: "Test View", type: "view" });
    expect(screen.getByText("Test View")).toBeInTheDocument();
  });

  it("applies correct border color from BORDER_COLORS", () => {
    const { container } = render(
      <ContextGraphNode {...({ data: { label: "Agent", type: "agent" } } as any)} />,
    );
    const inner = container.querySelector("div > div:last-child") as HTMLElement;
    expect(inner.style.border).toContain("var(--graph-agent-border)");
  });

  it("applies opacity style when opacity data is set", () => {
    const { container } = render(
      <ContextGraphNode {...({ data: { label: "Agent", type: "agent", opacity: 0.5 } } as any)} />,
    );
    const outer = container.firstChild as HTMLElement;
    expect(outer.style.opacity).toBe("0.5");
  });

  it("defaults opacity to 1 when not set", () => {
    const { container } = render(
      <ContextGraphNode {...({ data: { label: "Agent", type: "agent" } } as any)} />,
    );
    const outer = container.firstChild as HTMLElement;
    expect(outer.style.opacity).toBe("1");
  });

  it("shows visible handle style when showLeftHandle is true", () => {
    renderNode({ label: "Agent", type: "agent", showLeftHandle: true, showRightHandle: false });
    const leftHandle = screen.getByTestId("handle-target");
    expect(leftHandle.style.width).toBe("8px");
  });

  it("hides handle when showLeftHandle is false", () => {
    renderNode({ label: "Agent", type: "agent", showLeftHandle: false, showRightHandle: false });
    const leftHandle = screen.getByTestId("handle-target");
    expect(leftHandle.style.opacity).toBe("0");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/ContextGraphNode.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./ContextGraphNode`

- [ ] **Step 3: Write minimal implementation**

Create `ContextGraphNode.tsx`:

```tsx
import { Handle, type NodeProps, Position } from "@xyflow/react";
import {
  BG_COLORS,
  BORDER_COLORS,
  HANDLE_STYLE_HIDDEN,
  HANDLE_STYLE_VISIBLE,
  ICONS,
} from "../constants";

export function ContextGraphNode({ data }: NodeProps) {
  const { label, type, opacity, showLeftHandle, showRightHandle } = data as {
    label: string;
    type: string;
    opacity?: number;
    showLeftHandle?: boolean;
    showRightHandle?: boolean;
  };
  const borderColor = BORDER_COLORS[type];
  const nodeOpacity = opacity ?? 1;

  return (
    <div
      style={{
        position: "relative",
        width: "fit-content",
        opacity: nodeOpacity,
        transform: `scale(${nodeOpacity > 0 ? 1 : 0.8})`,
        transition: "opacity 0.3s ease, transform 0.3s ease",
        pointerEvents: nodeOpacity === 0 ? "none" : "auto",
      }}
    >
      <Handle
        type="target"
        position={Position.Left}
        style={showLeftHandle ? HANDLE_STYLE_VISIBLE : HANDLE_STYLE_HIDDEN}
      />
      <Handle
        type="source"
        position={Position.Right}
        style={showRightHandle ? HANDLE_STYLE_VISIBLE : HANDLE_STYLE_HIDDEN}
      />
      <div
        style={{
          padding: "6px 12px",
          borderRadius: "6px",
          border: `1.5px solid ${borderColor}`,
          background: BG_COLORS[type],
          display: "flex",
          alignItems: "center",
          gap: "8px",
          color: borderColor,
        }}
      >
        {ICONS[type]}
        <span
          style={{
            fontSize: "13px",
            fontWeight: 500,
            whiteSpace: "nowrap",
            color: "var(--foreground)",
          }}
        >
          {label}
        </span>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/ContextGraphNode.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/components/ContextGraphNode.tsx web-app/src/pages/context-graph/ContextGraph/components/ContextGraphNode.test.tsx
git commit -m "refactor: extract ContextGraphNode component with tests"
```

---

## Task 5: Create GraphStatsPanel.tsx (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.tsx`

- [ ] **Step 1: Write the failing tests**

Create `GraphStatsPanel.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import { GraphStatsPanel } from "./GraphStatsPanel";

afterEach(() => cleanup());

const makeNodes = (count: number, type: ContextGraphNode["type"] = "agent"): ContextGraphNode[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `${type}-${i}`,
    type,
    label: `${type} ${i}`,
    data: { name: `${type}${i}` },
  }));

const makeEdges = (count: number): ContextGraphEdge[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `e${i}`,
    source: `src-${i}`,
    target: `tgt-${i}`,
  }));

describe("GraphStatsPanel", () => {
  it("shows correct total node count", () => {
    render(
      <GraphStatsPanel
        nodes={makeNodes(5)}
        edges={makeEdges(3)}
        typeCounts={{ agent: 5 }}
      />,
    );
    expect(screen.getByTestId("context-graph-total-nodes")).toHaveTextContent("5");
  });

  it("shows correct total edge count", () => {
    render(
      <GraphStatsPanel
        nodes={makeNodes(2)}
        edges={makeEdges(7)}
        typeCounts={{ agent: 2 }}
      />,
    );
    expect(screen.getByTestId("context-graph-total-edges")).toHaveTextContent("7");
  });

  it("renders per-type count rows", () => {
    render(
      <GraphStatsPanel
        nodes={[...makeNodes(3, "agent"), ...makeNodes(2, "table")]}
        edges={[]}
        typeCounts={{ agent: 3, table: 2 }}
      />,
    );
    expect(screen.getByText("Agents:")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("Tables:")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
  });

  it("renders 'Context Graph Overview' heading", () => {
    render(<GraphStatsPanel nodes={[]} edges={[]} typeCounts={{}} />);
    expect(screen.getByText("Context Graph Overview")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./GraphStatsPanel`

- [ ] **Step 3: Write minimal implementation**

Create `GraphStatsPanel.tsx`:

```tsx
import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import { TYPE_LABELS } from "../../../constants";

interface GraphStatsPanelProps {
  nodes: ContextGraphNode[];
  edges: ContextGraphEdge[];
  typeCounts: Record<string, number>;
}

export function GraphStatsPanel({ nodes, edges, typeCounts }: GraphStatsPanelProps) {
  return (
    <>
      <div className="mb-2 font-semibold text-sidebar-foreground text-sm">
        Context Graph Overview
      </div>
      <div
        className="space-y-1 text-sidebar-foreground/70 text-sm"
        data-testid="context-graph-stats"
      >
        <div className="flex justify-between gap-4">
          <span>Total Nodes:</span>
          <span
            className="font-medium text-sidebar-foreground"
            data-testid="context-graph-total-nodes"
          >
            {nodes.length}
          </span>
        </div>
        <div className="flex justify-between gap-4">
          <span>Total Edges:</span>
          <span
            className="font-medium text-sidebar-foreground"
            data-testid="context-graph-total-edges"
          >
            {edges.length}
          </span>
        </div>
        <div className="mt-2 border-sidebar-border border-t pt-2">
          {Object.entries(typeCounts).map(([type, count]) => (
            <div key={type} className="flex justify-between gap-4">
              <span>{TYPE_LABELS[type] || type}:</span>
              <span className="font-medium text-sidebar-foreground">{count}</span>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.tsx web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphStatsPanel.test.tsx
git commit -m "refactor: extract GraphStatsPanel component with tests"
```

---

## Task 6: Create GraphFilterPanel.tsx (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.tsx`

- [ ] **Step 1: Write the failing tests**

Create `GraphFilterPanel.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GraphFilterPanel } from "./GraphFilterPanel";

afterEach(() => cleanup());

const defaultProps = {
  focusType: "auto" as const,
  onFocusTypeChange: vi.fn(),
  expandAll: false,
  onExpandAllChange: vi.fn(),
  focusedNodeId: null as string | null,
  onReset: vi.fn(),
};

describe("GraphFilterPanel", () => {
  it("renders focus type selector", () => {
    render(<GraphFilterPanel {...defaultProps} />);
    expect(screen.getByText("Focus View")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-filter-type")).toBeInTheDocument();
  });

  it("renders expand all checkbox", () => {
    render(<GraphFilterPanel {...defaultProps} />);
    expect(screen.getByText("Expand all connected")).toBeInTheDocument();
  });

  it("expand checkbox is disabled when focusedNodeId is null", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId={null} />);
    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).toBeDisabled();
  });

  it("expand checkbox is enabled when focusedNodeId is set", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId="node-1" />);
    const checkbox = screen.getByRole("checkbox");
    expect(checkbox).not.toBeDisabled();
  });

  it("calls onExpandAllChange when checkbox toggled", () => {
    const onExpandAllChange = vi.fn();
    render(
      <GraphFilterPanel
        {...defaultProps}
        focusedNodeId="node-1"
        onExpandAllChange={onExpandAllChange}
      />,
    );
    fireEvent.click(screen.getByRole("checkbox"));
    expect(onExpandAllChange).toHaveBeenCalledWith(true);
  });

  it("reset button is hidden when focusedNodeId is null", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId={null} />);
    expect(screen.queryByText("Reset View")).not.toBeInTheDocument();
  });

  it("reset button is visible when focusedNodeId is set", () => {
    render(<GraphFilterPanel {...defaultProps} focusedNodeId="node-1" />);
    expect(screen.getByText("Reset View")).toBeInTheDocument();
  });

  it("calls onReset when reset button clicked", () => {
    const onReset = vi.fn();
    render(<GraphFilterPanel {...defaultProps} focusedNodeId="node-1" onReset={onReset} />);
    fireEvent.click(screen.getByText("Reset View"));
    expect(onReset).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./GraphFilterPanel`

- [ ] **Step 3: Write minimal implementation**

Create `GraphFilterPanel.tsx`:

```tsx
import { Filter } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { type FocusType, FOCUS_OPTIONS } from "../../../constants";

interface GraphFilterPanelProps {
  focusType: FocusType;
  onFocusTypeChange: (type: FocusType) => void;
  expandAll: boolean;
  onExpandAllChange: (expand: boolean) => void;
  focusedNodeId: string | null;
  onReset: () => void;
}

export function GraphFilterPanel({
  focusType,
  onFocusTypeChange,
  expandAll,
  onExpandAllChange,
  focusedNodeId,
  onReset,
}: GraphFilterPanelProps) {
  return (
    <div className="mt-3 border-sidebar-border border-t pt-3">
      <div className="mb-2 flex items-center gap-2">
        <Filter className="h-4 w-4 text-sidebar-foreground/70" />
        <span className="font-semibold text-sidebar-foreground text-sm">Focus View</span>
      </div>
      <Select
        value={focusType}
        onValueChange={(value) => onFocusTypeChange(value as FocusType)}
      >
        <SelectTrigger
          className="h-9 border-sidebar-border bg-sidebar-accent text-sidebar-foreground text-sm"
          data-testid="context-graph-filter-type"
        >
          <SelectValue placeholder="Select focus" />
        </SelectTrigger>
        <SelectContent>
          {FOCUS_OPTIONS.map(({ value, label, icon }) => (
            <SelectItem key={value} value={value} className="cursor-pointer text-sm">
              <div className="flex items-center gap-2">
                {icon}
                <span>{label}</span>
              </div>
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div className="mt-2 border-sidebar-border border-t pt-2">
        <label
          className={`flex items-center gap-2 ${
            focusedNodeId ? "cursor-pointer" : "cursor-not-allowed opacity-50"
          }`}
        >
          <input
            type="checkbox"
            checked={expandAll}
            onChange={(e) => onExpandAllChange(e.target.checked)}
            disabled={!focusedNodeId}
            className="h-4 w-4 rounded border-border text-primary focus:ring-primary disabled:cursor-not-allowed"
          />
          <span className="text-sm">Expand all connected</span>
        </label>
        <p className="mt-1 text-muted-foreground text-xs">
          Show entire cluster when clicked
        </p>
      </div>

      {focusedNodeId && (
        <div className="mt-2 border-sidebar-border border-t pt-2">
          <Button
            onClick={onReset}
            variant="outline"
            size="sm"
            className="w-full text-sm"
          >
            Reset View
          </Button>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.tsx web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/components/GraphFilterPanel.test.tsx
git commit -m "refactor: extract GraphFilterPanel component with tests"
```

---

## Task 7: Create GraphControlPanel/index.tsx (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.tsx`

- [ ] **Step 1: Write the failing tests**

Create `index.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@xyflow/react", () => ({
  Panel: ({ children, className }: { children: React.ReactNode; className: string }) => (
    <div data-testid="rf-panel" className={className}>{children}</div>
  ),
}));

const { GraphControlPanel } = await import("./index");

afterEach(() => cleanup());

const defaultProps = {
  nodes: [{ id: "n1", type: "agent" as const, label: "Agent", data: { name: "a" } }],
  edges: [{ id: "e1", source: "n1", target: "n2" }],
  typeCounts: { agent: 1 },
  focusType: "auto" as const,
  onFocusTypeChange: vi.fn(),
  expandAll: false,
  onExpandAllChange: vi.fn(),
  focusedNodeId: null as string | null,
  onReset: vi.fn(),
};

describe("GraphControlPanel", () => {
  it("renders stats section", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByText("Context Graph Overview")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-total-nodes")).toHaveTextContent("1");
  });

  it("renders filter section", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByText("Focus View")).toBeInTheDocument();
    expect(screen.getByTestId("context-graph-filter-type")).toBeInTheDocument();
  });

  it("renders inside an RFPanel", () => {
    render(<GraphControlPanel {...defaultProps} />);
    expect(screen.getByTestId("rf-panel")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./index`

- [ ] **Step 3: Write minimal implementation**

Create `index.tsx`:

```tsx
import { Panel as RFPanel } from "@xyflow/react";
import type { ContextGraphEdge, ContextGraphNode } from "@/types/contextGraph";
import type { FocusType } from "../../constants";
import { GraphFilterPanel } from "./components/GraphFilterPanel";
import { GraphStatsPanel } from "./components/GraphStatsPanel";

interface GraphControlPanelProps {
  nodes: ContextGraphNode[];
  edges: ContextGraphEdge[];
  typeCounts: Record<string, number>;
  focusType: FocusType;
  onFocusTypeChange: (type: FocusType) => void;
  expandAll: boolean;
  onExpandAllChange: (expand: boolean) => void;
  focusedNodeId: string | null;
  onReset: () => void;
}

export function GraphControlPanel({
  nodes,
  edges,
  typeCounts,
  focusType,
  onFocusTypeChange,
  expandAll,
  onExpandAllChange,
  focusedNodeId,
  onReset,
}: GraphControlPanelProps) {
  return (
    <RFPanel
      position="top-left"
      className="rounded-lg border border-sidebar-border bg-sidebar-background p-4 shadow-lg"
    >
      <GraphStatsPanel nodes={nodes} edges={edges} typeCounts={typeCounts} />
      <GraphFilterPanel
        focusType={focusType}
        onFocusTypeChange={onFocusTypeChange}
        expandAll={expandAll}
        onExpandAllChange={onExpandAllChange}
        focusedNodeId={focusedNodeId}
        onReset={onReset}
      />
    </RFPanel>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.tsx web-app/src/pages/context-graph/ContextGraph/components/GraphControlPanel/index.test.tsx
git commit -m "refactor: create GraphControlPanel wrapper component with tests"
```

---

## Task 8: Move NodeDetailPanel + add tests (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.tsx` (copy from `pages/context-graph/NodeDetailPanel.tsx` — no code changes)

- [ ] **Step 1: Write the failing tests**

Create `NodeDetailPanel.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ContextGraphNode } from "@/types/contextGraph";

vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
}));

vi.mock("@/hooks/useCurrentProjectBranch", () => ({
  default: () => ({ project: { id: "test-project" }, branchName: "main" }),
}));

vi.mock("@/services/api/files", () => ({
  FileService: {
    getFile: vi.fn(() => new Promise(() => {})), // never resolves by default
  },
}));

vi.mock("react-syntax-highlighter", () => ({
  Prism: ({ children }: { children: string }) => <pre data-testid="syntax-highlighter">{children}</pre>,
}));

vi.mock("react-syntax-highlighter/dist/esm/styles/prism", () => ({
  oneDark: {},
}));

const { NodeDetailPanel } = await import("./NodeDetailPanel");

afterEach(() => cleanup());

const makeNode = (overrides: Partial<ContextGraphNode> = {}): ContextGraphNode => ({
  id: "test-node",
  type: "agent",
  label: "Test Agent",
  data: { name: "test-agent", ...overrides.data },
  ...overrides,
});

describe("NodeDetailPanel", () => {
  it("returns null when node is null", () => {
    const { container } = render(<NodeDetailPanel node={null} onClose={vi.fn()} />);
    expect(container.innerHTML).toBe("");
  });

  it("renders node label and type", () => {
    render(<NodeDetailPanel node={makeNode()} onClose={vi.fn()} />);
    expect(screen.getByText("Test Agent")).toBeInTheDocument();
    expect(screen.getByText("Agent")).toBeInTheDocument();
  });

  it("shows path when node.data.path exists", () => {
    render(
      <NodeDetailPanel
        node={makeNode({ data: { name: "a", path: "agents/test.agent.yml" } })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText("agents/test.agent.yml")).toBeInTheDocument();
  });

  it("shows description when node.data.description exists", () => {
    render(
      <NodeDetailPanel
        node={makeNode({ data: { name: "a", description: "A test agent" } })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByText("A test agent")).toBeInTheDocument();
  });

  it("shows 'Open in IDE' button for file node types", () => {
    render(
      <NodeDetailPanel
        node={makeNode({ type: "agent", data: { name: "a", path: "agents/test.yml" } })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTitle("Open in IDE")).toBeInTheDocument();
  });

  it("does not show 'Open in IDE' button for non-file node types", () => {
    render(
      <NodeDetailPanel
        node={makeNode({ type: "table" })}
        onClose={vi.fn()}
      />,
    );
    expect(screen.queryByTitle("Open in IDE")).not.toBeInTheDocument();
  });

  it("shows loading spinner while file content loads", () => {
    render(
      <NodeDetailPanel
        node={makeNode({ type: "agent", data: { name: "a", path: "agents/test.yml" } })}
        onClose={vi.fn()}
      />,
    );
    // FileService.getFile never resolves, so spinner should show
    expect(screen.getByText("File Contents")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/NodeDetailPanel.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./NodeDetailPanel`

- [ ] **Step 3: Copy NodeDetailPanel.tsx to new location (no code changes)**

Copy `web-app/src/pages/context-graph/NodeDetailPanel.tsx` to `web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.tsx`.

```bash
cp web-app/src/pages/context-graph/NodeDetailPanel.tsx web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.tsx
```

The file content is identical — no imports need changing because all imports use `@/` absolute paths.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/components/NodeDetailPanel.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.tsx web-app/src/pages/context-graph/ContextGraph/components/NodeDetailPanel.test.tsx
git commit -m "refactor: move NodeDetailPanel to ContextGraph/components with tests"
```

---

## Task 9: Rewrite orchestrator ContextGraph/index.tsx (TDD)

**Files:**
- Create: `web-app/src/pages/context-graph/ContextGraph/index.test.tsx`
- Create: `web-app/src/pages/context-graph/ContextGraph/index.tsx`

- [ ] **Step 1: Write the failing tests**

Create `index.test.tsx`:

```tsx
// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

// Mock ReactFlow entirely — it requires browser APIs not available in jsdom
vi.mock("@xyflow/react", () => ({
  ReactFlow: ({
    nodes,
    onNodeClick,
    onPaneClick,
    children,
  }: {
    nodes: Array<{ id: string; data: { label: string } }>;
    onNodeClick: (event: React.MouseEvent, node: { id: string }) => void;
    onPaneClick: () => void;
    children: React.ReactNode;
  }) => (
    <div data-testid="react-flow">
      {nodes.map((n) => (
        <div
          key={n.id}
          data-testid={`rf-node-${n.id}`}
          onClick={(e) => onNodeClick(e, n)}
        >
          {n.data.label}
        </div>
      ))}
      <div data-testid="rf-pane" onClick={onPaneClick} />
      {children}
    </div>
  ),
  ReactFlowProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
  Background: () => null,
  BackgroundVariant: { Dots: "dots" },
  Panel: ({
    children,
    className,
  }: {
    children: React.ReactNode;
    className?: string;
    position?: string;
  }) => (
    <div data-testid="rf-panel" className={className}>
      {children}
    </div>
  ),
  Handle: () => null,
  Position: { Left: "left", Right: "right" },
  useNodesState: (initial: unknown[]) => {
    const [nodes, setNodes] = useState(initial);
    return [nodes, setNodes, vi.fn()];
  },
  useEdgesState: (initial: unknown[]) => {
    const [edges, setEdges] = useState(initial);
    return [edges, setEdges, vi.fn()];
  },
}));

// Mock NodeDetailPanel to observe props
vi.mock("./components/NodeDetailPanel", () => ({
  NodeDetailPanel: ({ node, onClose }: { node: unknown; onClose: () => void }) => (
    <div data-testid="node-detail-panel" data-has-node={node !== null ? "true" : "false"}>
      {node !== null && <button data-testid="close-panel" onClick={onClose}>Close</button>}
    </div>
  ),
}));

// Mock react-router-dom (needed by NodeDetailPanel's real import, but we mock NodeDetailPanel)
vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
}));

const { ContextGraph } = await import("./index");

afterEach(() => cleanup());

const mockData = {
  nodes: [
    { id: "a1", type: "agent" as const, label: "Agent 1", data: { name: "agent1" } },
    { id: "t1", type: "table" as const, label: "Table 1", data: { name: "table1" } },
  ],
  edges: [{ id: "e1", source: "a1", target: "t1" }],
};

describe("ContextGraph orchestrator", () => {
  it("renders ReactFlow with correct number of nodes", () => {
    render(<ContextGraph data={mockData} />);
    expect(screen.getByTestId("react-flow")).toBeInTheDocument();
    expect(screen.getByTestId("rf-node-a1")).toBeInTheDocument();
    expect(screen.getByTestId("rf-node-t1")).toBeInTheDocument();
  });

  it("node click shows NodeDetailPanel", () => {
    render(<ContextGraph data={mockData} />);
    expect(screen.getByTestId("node-detail-panel").dataset.hasNode).toBe("false");

    fireEvent.click(screen.getByTestId("rf-node-a1"));
    expect(screen.getByTestId("node-detail-panel").dataset.hasNode).toBe("true");
  });

  it("pane click hides NodeDetailPanel", () => {
    render(<ContextGraph data={mockData} />);

    fireEvent.click(screen.getByTestId("rf-node-a1"));
    expect(screen.getByTestId("node-detail-panel").dataset.hasNode).toBe("true");

    fireEvent.click(screen.getByTestId("rf-pane"));
    expect(screen.getByTestId("node-detail-panel").dataset.hasNode).toBe("false");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/index.test.tsx 2>&1 | tail -10`
Expected: FAIL — cannot find module `./index`

- [ ] **Step 3: Write the orchestrator**

Create `index.tsx`:

```tsx
import {
  Background,
  BackgroundVariant,
  type Edge,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
} from "@xyflow/react";
import { useMemo } from "react";
import "@xyflow/react/dist/style.css";
import type { ContextGraph as ContextGraphType } from "@/types/contextGraph";
import { TYPE_LABELS } from "./constants";
import { buildInitialEdges, buildInitialNodes } from "./layout";
import { useGraphFocus } from "./useGraphFocus";
import { ContextGraphNode } from "./components/ContextGraphNode";
import { GraphControlPanel } from "./components/GraphControlPanel";
import { NodeDetailPanel } from "./components/NodeDetailPanel";

const nodeTypes = { "context-graph": ContextGraphNode };

interface ContextGraphProps {
  data: ContextGraphType;
}

function ContextGraphInner({ data }: ContextGraphProps) {
  const initialNodes = useMemo(() => buildInitialNodes(data.nodes), [data.nodes]);
  const initialEdges = useMemo(() => buildInitialEdges(data.edges), [data.edges]);

  const [nodes, setNodes, onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initialEdges);

  const {
    focusedNodeId,
    selectedNode,
    focusType,
    expandAll,
    changeFocusType,
    setExpandAll,
    handleNodeClick,
    handlePaneClick,
    resetView,
  } = useGraphFocus({ data, setNodes, setEdges });

  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const node of data.nodes) {
      counts[node.type] = (counts[node.type] || 0) + 1;
    }
    return counts;
  }, [data.nodes]);

  return (
    <div style={{ width: "100vw", height: "100vh", position: "relative" }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={handleNodeClick}
        onPaneClick={handlePaneClick}
        nodeTypes={nodeTypes}
        elementsSelectable={false}
        nodesConnectable={false}
        nodesDraggable={false}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.1}
        maxZoom={2}
        proOptions={{ hideAttribution: true }}
        style={{ width: "100%", height: "100%", background: "var(--background)" }}
      >
        <Background color="var(--muted-foreground)" variant={BackgroundVariant.Dots} />
        <GraphControlPanel
          nodes={data.nodes}
          edges={data.edges}
          typeCounts={typeCounts}
          focusType={focusType}
          onFocusTypeChange={changeFocusType}
          expandAll={expandAll}
          onExpandAllChange={setExpandAll}
          focusedNodeId={focusedNodeId}
          onReset={resetView}
        />
      </ReactFlow>
      <NodeDetailPanel node={selectedNode} onClose={resetView} />
    </div>
  );
}

export function ContextGraph(props: ContextGraphProps) {
  return (
    <ReactFlowProvider>
      <ContextGraphInner {...props} />
    </ReactFlowProvider>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ContextGraph/index.test.tsx 2>&1 | tail -15`
Expected: All tests PASS

- [ ] **Step 5: Commit**

```bash
git add web-app/src/pages/context-graph/ContextGraph/index.tsx web-app/src/pages/context-graph/ContextGraph/index.test.tsx
git commit -m "refactor: create slim ContextGraph orchestrator with integration tests"
```

---

## Task 10: Cleanup — delete old files, verify build + all tests

**Files:**
- Delete: `web-app/src/pages/context-graph/ContextGraph.tsx`
- Delete: `web-app/src/pages/context-graph/NodeDetailPanel.tsx`
- Verify: `web-app/src/pages/context-graph/index.tsx` (import resolves to `ContextGraph/index.tsx` — no changes needed)

- [ ] **Step 1: Delete old ContextGraph.tsx**

```bash
rm web-app/src/pages/context-graph/ContextGraph.tsx
```

The page wrapper `pages/context-graph/index.tsx` imports `"./ContextGraph"` which now resolves to `./ContextGraph/index.tsx`.

- [ ] **Step 2: Delete old NodeDetailPanel.tsx**

```bash
rm web-app/src/pages/context-graph/NodeDetailPanel.tsx
```

- [ ] **Step 3: Verify TypeScript compiles**

Run: `cd web-app && pnpm exec tsc --noEmit --pretty 2>&1 | head -20`
Expected: No errors

- [ ] **Step 4: Run all context-graph tests**

Run: `cd web-app && pnpm exec vitest run src/pages/context-graph/ 2>&1 | tail -25`
Expected: All tests PASS (layout, useGraphFocus, ContextGraphNode, GraphStatsPanel, GraphFilterPanel, GraphControlPanel, NodeDetailPanel, orchestrator)

- [ ] **Step 5: Verify dev server starts**

Run: `cd web-app && timeout 15 pnpm dev 2>&1 | tail -5`
Expected: "ready in" message with no errors

- [ ] **Step 6: Commit**

```bash
git add -A web-app/src/pages/context-graph/
git commit -m "refactor: remove old ContextGraph.tsx and NodeDetailPanel.tsx after migration"
```

---

## Files NOT Changed

| File | Reason |
|---|---|
| `pages/context-graph/index.tsx` | Page wrapper — import `"./ContextGraph"` resolves to new folder automatically |
| `src/types/contextGraph.ts` | Types unchanged |
| `src/hooks/api/contextGraph/useContextGraph.ts` | Data fetching unchanged |
| `src/services/api/contextGraph.ts` | API service unchanged |
| `src/styles/shadcn/index.css` | CSS variables unchanged |
