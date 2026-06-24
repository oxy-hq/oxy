import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  type Edge as RFEdge,
  type Node as RFNode
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useState } from "react";
import type { WmComputedMeasure, WmSelection, WorldModel } from "@/types/worldModel";
import {
  EXPANDED_NODE_WIDTH,
  layoutWithElk,
  NODE_HEIGHT_COLLAPSED,
  NODE_WIDTH,
  type WaypointMap,
  worldModelToFlow
} from "../worldModelLayout";
import { WorldModelEdge } from "./WorldModelEdge";
import { WorldModelEntityNode } from "./WorldModelEntityNode";
import { WorldModelExpandedEntityNode } from "./WorldModelExpandedEntityNode";

const nodeTypes = {
  "wm-entity": WorldModelEntityNode,
  "wm-entity-expanded": WorldModelExpandedEntityNode
};
const edgeTypes = { "wm-edge": WorldModelEdge };

interface WorldModelGraphProps {
  model: WorldModel;
  selection: WmSelection;
  filterCounts?: Record<string, { matched: number; total: number }> | null;
  isCountLoading?: boolean;
  filterSeedEntityId?: string | null;
  seedComputedMeasures?: WmComputedMeasure[] | null;
  expandedEntityId?: string | null;
  breakdownMeasure?: string | null;
  instanceKey?: string | null;
  onExpandEntity?: (id: string | null, measure?: string | null) => void;
  onSelectEntity: (id: string) => void;
  onSelectPromotion: (from: string, to: string) => void;
  onOpenPicker: (entityId: string, pos: { x: number; y: number }) => void;
  onClearSelection: () => void;
}

export function WorldModelGraph({
  model,
  selection,
  filterCounts,
  isCountLoading = false,
  filterSeedEntityId = null,
  seedComputedMeasures = null,
  expandedEntityId = null,
  breakdownMeasure = null,
  instanceKey = null,
  onExpandEntity,
  onSelectEntity,
  onSelectPromotion,
  onOpenPicker,
  onClearSelection
}: WorldModelGraphProps) {
  // Unconnected entities are always hidden — the graph only shows entities
  // that participate in at least one relationship.
  const filteredModel = useMemo<WorldModel>(() => {
    const connected = new Set<string>();
    for (const e of model.edges) {
      connected.add(e.from);
      connected.add(e.to);
    }
    return { ...model, entities: model.entities.filter((n) => connected.has(n.id)) };
  }, [model]);

  // Topology-only layout trigger — selection changes don't re-run ELK.
  const { nodes: rawNodes, edges: layoutEdges } = useMemo(
    () => worldModelToFlow(filteredModel, null),
    [filteredModel]
  );

  // Selection-aware overlay — instant, no ELK call.
  const { nodes: selectionNodes, edges } = useMemo(
    () => worldModelToFlow(filteredModel, selection),
    [filteredModel, selection]
  );

  const selectionDataMap = useMemo(
    () => new Map(selectionNodes.map((n) => [n.id, n.data])),
    [selectionNodes]
  );

  const [positioned, setPositioned] = useState<RFNode[] | null>(null);
  const [waypointMap, setWaypointMap] = useState<WaypointMap>(new Map());

  useEffect(() => {
    let cancelled = false;
    setPositioned(null);
    layoutWithElk(rawNodes, layoutEdges)
      .then(({ nodes: laidOut, waypointMap: wm }) => {
        if (!cancelled) {
          setPositioned(laidOut);
          setWaypointMap(wm);
        }
      })
      .catch((err) => {
        console.error("world model layout failed", err);
        if (!cancelled) {
          setPositioned(rawNodes);
          setWaypointMap(new Map());
        }
      });
    return () => {
      cancelled = true;
    };
  }, [rawNodes, layoutEdges]);

  // Derive the currently highlighted entity id from selection
  const selectedEntityId =
    selection?.kind === "entity"
      ? selection.entityId
      : selection?.kind === "dimension" || selection?.kind === "measure"
        ? selection.entityId
        : selection?.kind === "instance"
          ? selection.entityId
          : null;

  // Merge: positions from layout + selection state + filter counts + chip data.
  const isInstanceSelection = selection?.kind === "instance";
  const displayNodes = useMemo(
    () =>
      positioned?.map((n) => {
        const selData = selectionDataMap.get(n.id) ?? n.data;
        const isSeed = n.id === filterSeedEntityId;
        const isExpanded = n.id === expandedEntityId;

        // Expanded node: swap to the driver-tree card, widen, raise z, inject breakdown.
        if (isExpanded) {
          return {
            ...n,
            type: "wm-entity-expanded",
            width: EXPANDED_NODE_WIDTH,
            height: undefined,
            zIndex: 10,
            data: {
              ...selData,
              seedComputedMeasures,
              breakdownMeasure,
              instanceKey,
              onExpandEntity,
              dimmed: false
            }
          };
        }

        // Other nodes drop back when any expansion is active, focusing the card.
        const dimmed = expandedEntityId
          ? true
          : isInstanceSelection
            ? n.id !== selectedEntityId && filterCounts?.[n.id] === undefined
            : (selData.dimmed as boolean);

        return {
          ...n,
          type: "wm-entity",
          width: NODE_WIDTH,
          height: NODE_HEIGHT_COLLAPSED,
          data: {
            ...selData,
            filterCount: filterCounts?.[n.id] ?? null,
            isCountLoading,
            dimmed,
            seedComputedMeasures: isSeed ? seedComputedMeasures : null,
            onExpandEntity
          }
        };
      }) ?? null,
    [
      positioned,
      selectionDataMap,
      filterCounts,
      isCountLoading,
      isInstanceSelection,
      selectedEntityId,
      filterSeedEntityId,
      seedComputedMeasures,
      expandedEntityId,
      breakdownMeasure,
      instanceKey,
      onExpandEntity
    ]
  );

  // Overlay edge opacity to match progressive node reveal during instance selection.
  // Embed Graphviz-computed SVG paths so edges route around node bodies.
  const displayEdges = useMemo(() => {
    return edges.map((e) => {
      const opacity = isInstanceSelection
        ? filterCounts?.[e.source] !== undefined && filterCounts?.[e.target] !== undefined
          ? 0.6
          : 0.08
        : (e.style?.opacity as number | undefined);
      return {
        ...e,
        type: "wm-edge",
        data: { ...e.data, waypoints: waypointMap.get(e.id) },
        style: { ...e.style, ...(opacity !== undefined ? { opacity } : {}) }
      };
    });
  }, [edges, filterCounts, isInstanceSelection, waypointMap]);

  const handleNodeClick = (event: React.MouseEvent, node: RFNode) => {
    // The expanded card owns its own interactions (close button, tree); ignore body clicks.
    if (node.id === expandedEntityId) return;
    // First click both selects the entity and opens its instance picker — no
    // second click required.
    onSelectEntity(node.id);
    onOpenPicker(node.id, { x: event.clientX, y: event.clientY });
  };

  const handleEdgeClick = (_event: React.MouseEvent, edge: RFEdge) => {
    onSelectPromotion(edge.source, edge.target);
  };

  if (model.entities.length === 0) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
        No relationships found. Add <code className='mx-1 font-mono text-xs'>parent:</code> or
        foreign entity declarations to your .view.yml files.
      </div>
    );
  }

  return (
    <div className='relative h-full w-full'>
      {filteredModel.entities.length === 0 ? (
        <div className='flex h-full flex-col items-center justify-center gap-1 text-muted-foreground text-sm'>
          <p>All entities are unconnected.</p>
          <p className='text-xs'>Add a relationship to see them in the graph.</p>
        </div>
      ) : displayNodes === null ? (
        <div className='flex h-full items-center justify-center text-muted-foreground text-xs'>
          Laying out…
        </div>
      ) : (
        <ReactFlow
          key={`${filteredModel.entities.length}-${edges.length}`}
          nodes={displayNodes}
          edges={displayEdges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodeClick={handleNodeClick}
          onEdgeClick={handleEdgeClick}
          onPaneClick={onClearSelection}
          fitView
          fitViewOptions={{ padding: 0.16 }}
          minZoom={0.3}
          maxZoom={1.8}
          proOptions={{ hideAttribution: true }}
          style={{ background: "var(--background)" }}
        >
          <Background variant={BackgroundVariant.Dots} color='var(--border)' gap={22} size={1} />
          <Controls showInteractive={false} className='rounded-lg border border-border shadow-sm' />
        </ReactFlow>
      )}
    </div>
  );
}
