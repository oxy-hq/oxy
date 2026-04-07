# Context Graph Refactor — Code Organization + Tests

## Goal

Refactor `ContextGraph.tsx` (~640 lines) into smaller, well-organized files following the project's recursive colocation convention. Add Vitest component/unit tests for each extracted module. All existing features preserved — no behavior changes.

## File Structure

```
pages/context-graph/
├── index.tsx                              ← page wrapper (unchanged)
└── ContextGraph/
    ├── index.tsx                          ← slim orchestrator
    ├── index.test.tsx                     ← integration test
    ├── constants.ts                       ← shared color maps, icons, labels, options
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

## Extraction Map

### `constants.ts`

Extracted from `ContextGraph.tsx` lines 42–226. Contains:

- `BORDER_COLORS` — border color CSS variables per node type
- `BG_COLORS` — background color CSS variables per node type
- `HANDLE_STYLE_HIDDEN`, `HANDLE_STYLE_VISIBLE` — ReactFlow handle styles
- `ICONS` — icon JSX per node type
- `TYPE_ORDER` — layout ordering of node types
- `TYPE_LABELS` — display labels per node type
- `FOCUS_OPTIONS` — focus selector dropdown options
- `FocusType` — union type for focus filter
- `ROW_HEIGHT`, `MIN_NODE_WIDTH`, `PADDING`, `MAX_ROW_WIDTH` — layout constants

Shared across `ContextGraphNode`, `GraphStatsPanel`, `GraphFilterPanel`, `layout.ts`, and the orchestrator.

### `layout.ts`

Extracted from `ContextGraph.tsx` lines 274–335 and 614–632. Contains:

- `layoutRow(row, rowIndex)` — positions a row of nodes horizontally, centered
- `buildInitialNodes(nodes)` — groups nodes by type, wraps into rows, returns `Node[]`
- `buildInitialEdges(edges)` — maps domain edges to ReactFlow edges with default styling

Pure functions, no React dependencies.

### `useGraphFocus.ts`

Extracted from `ContextGraph.tsx` lines 229–266 and 341–466. Contains:

- `focusedNodeId`, `selectedNode`, `focusType`, `expandAll` state
- localStorage persistence for `focusType` and `expandAll`
- `neighbors` adjacency map (built from edges via `useMemo`)
- `getConnectedNodes(startIds, maxDepth?)` — BFS traversal
- `focusTypeVisible` — set of visible node IDs for type filter
- `useEffect` that updates node/edge styling when focus state changes
- `handleNodeClick`, `handlePaneClick` callbacks

Takes `data`, `setNodes`, `setEdges` as parameters. Returns state + callbacks consumed by the orchestrator and passed to child components.

### `components/ContextGraphNode.tsx`

Extracted from `ContextGraph.tsx` lines 100–160. The custom ReactFlow node component.

- Reads `label`, `type`, `opacity`, `showLeftHandle`, `showRightHandle` from `data`
- Renders colored border/background, icon, label text
- Shows/hides handles based on focus state

### `components/GraphControlPanel/index.tsx`

New wrapper component. Renders the `<RFPanel position="top-left">` container and composes `GraphStatsPanel` + `GraphFilterPanel` inside it.

### `components/GraphControlPanel/components/GraphStatsPanel.tsx`

Extracted from `ContextGraph.tsx` lines 497–534.

Props: `nodes`, `edges`, `typeCounts`

Renders total node/edge counts and per-type breakdown.

### `components/GraphControlPanel/components/GraphFilterPanel.tsx`

Extracted from `ContextGraph.tsx` lines 536–600.

Props: `focusType`, `onFocusTypeChange`, `expandAll`, `onExpandAllChange`, `focusedNodeId`, `onReset`

Renders focus type `<Select>`, expand-all checkbox, and reset button.

### `components/NodeDetailPanel.tsx`

Moved from `context-graph/NodeDetailPanel.tsx`. No code changes — just a new location.

### `ContextGraph/index.tsx` (orchestrator)

What remains after extraction (~80 lines):

- Imports hooks + components
- Calls `buildInitialNodes`, `buildInitialEdges`
- Calls `useNodesState`, `useEdgesState`
- Calls `useGraphFocus` with state setters
- Computes `typeCounts`
- Renders `<ReactFlow>` with `<Background>`, `<GraphControlPanel>`, `<NodeDetailPanel>`
- Wraps in `<ReactFlowProvider>`

## Test Plan

All tests use Vitest + `@testing-library/react`. Use `@vitest-environment jsdom` pragma for component tests.

### `layout.test.ts` — pure function tests

- `layoutRow` positions nodes with correct x/y offsets
- `layoutRow` centers nodes horizontally (negative x start)
- `buildInitialNodes` groups by type in `TYPE_ORDER`
- `buildInitialNodes` overflows into multiple rows when exceeding `MAX_ROW_WIDTH`
- `buildInitialEdges` maps edges with correct default styling

### `useGraphFocus.test.ts` — hook tests via `renderHook`

- BFS `getConnectedNodes` returns direct neighbors at depth 1
- BFS with no depth limit returns full connected cluster
- `focusTypeVisible` returns correct node set for a given type
- `handleNodeClick` toggles `focusedNodeId` on/off
- `handlePaneClick` clears both `focusedNodeId` and `selectedNode`

### `ContextGraphNode.test.tsx`

- Renders label text
- Renders correct icon for node type
- Applies correct border color from `BORDER_COLORS`
- Applies opacity style when `opacity` data is set
- Shows/hides handles based on `showLeftHandle`/`showRightHandle`

### `GraphControlPanel/index.test.tsx`

- Renders both stats and filter sections

### `GraphStatsPanel.test.tsx`

- Shows correct total node count
- Shows correct total edge count
- Renders per-type count rows

### `GraphFilterPanel.test.tsx`

- Renders focus type selector with all options
- Calls `onFocusTypeChange` when selection changes
- Expand checkbox disabled when `focusedNodeId` is null
- Expand checkbox enabled when `focusedNodeId` is set
- Reset button visible only when `focusedNodeId` is set
- Calls `onReset` when reset button clicked

### `NodeDetailPanel.test.tsx`

- Returns null when `node` is null
- Renders node label and type
- Shows path when `node.data.path` exists
- Shows description when `node.data.description` exists
- Shows "Open in IDE" button for file node types
- Shows loading spinner while file content loads

### `ContextGraph/index.test.tsx` — integration test

- Renders ReactFlow with correct number of nodes
- Node click shows NodeDetailPanel
- Pane click hides NodeDetailPanel

## Files NOT Changed

| File | Reason |
|---|---|
| `pages/context-graph/index.tsx` | Page wrapper with loading/error/empty states — stays as-is |
| `src/types/contextGraph.ts` | Types unchanged |
| `src/hooks/api/contextGraph/useContextGraph.ts` | Data fetching unchanged |
| `src/services/api/contextGraph.ts` | API service unchanged |
| `src/styles/shadcn/index.css` | CSS variables unchanged |
