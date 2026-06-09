import {
  Background,
  BackgroundVariant,
  type ColorMode,
  Controls,
  Handle,
  type NodeProps,
  Position,
  ReactFlow,
  type Node as RFNode
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useEffect, useMemo, useState } from "react";
import { Label } from "@/components/ui/shadcn/label";
import { Switch } from "@/components/ui/shadcn/switch";
import { cn } from "@/libs/shadcn/utils";
import useTheme from "@/stores/useTheme";
import type { MetricNode, MetricTree } from "@/types/metricTree";
import { layoutWithElk, metricTreeToFlow, NODE_WIDTH } from "../graphLayout";

type NodeRole = "composite" | "component" | "driver" | "leaf";

interface MetricMeasureData {
  node: MetricNode;
  selected: boolean;
  role: NodeRole;
}

const ROLE_STYLES: Record<
  NodeRole,
  { border: string; bg: string; badge: string; badgeText: string }
> = {
  composite: {
    border: "border-primary/60",
    bg: "bg-primary/5",
    badge: "bg-primary/15 text-primary",
    badgeText: "Composite"
  },
  component: {
    border: "border-success/50",
    bg: "bg-success/5",
    badge: "bg-success/15 text-success",
    badgeText: "Component"
  },
  driver: {
    border: "border-amber-500/50",
    bg: "bg-amber-500/5",
    badge: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
    badgeText: "Driver"
  },
  leaf: {
    border: "border-border",
    bg: "bg-card",
    badge: "bg-muted text-muted-foreground",
    badgeText: ""
  }
};

function MetricMeasureNode({ data }: NodeProps) {
  const { node, selected, role } = data as unknown as MetricMeasureData;
  const styles = ROLE_STYLES[role];

  return (
    <>
      <Handle type='target' position={Position.Top} className='opacity-0' />
      <div
        className={cn(
          "flex flex-col justify-center gap-1 rounded-xl border px-3 py-2.5 shadow-sm transition-all duration-150",
          styles.border,
          styles.bg,
          selected && "ring-2 ring-primary/50 ring-offset-1 ring-offset-background"
        )}
        style={{ width: NODE_WIDTH }}
        data-testid={`metric-node-${node.id}`}
      >
        <p
          className='truncate font-semibold text-foreground text-sm leading-tight'
          title={node.label}
        >
          {node.label}
        </p>
        <div className='flex items-center gap-1.5'>
          <span
            className={cn(
              "rounded px-1.5 py-0.5 font-medium text-[10px] leading-none",
              styles.badge
            )}
          >
            {styles.badgeText || node.measure_type}
          </span>
          {role !== "composite" && (
            <span className='truncate text-[10px] text-muted-foreground' title={node.measure}>
              {node.measure}
            </span>
          )}
        </div>
      </div>
      <Handle type='source' position={Position.Bottom} className='opacity-0' />
    </>
  );
}

const nodeTypes = { "metric-measure": MetricMeasureNode };

interface MetricTreeGraphProps {
  tree: MetricTree;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

function deriveNodeRoles(tree: MetricTree): Map<string, NodeRole> {
  const roles = new Map<string, NodeRole>();
  const componentTargets = new Set(
    tree.edges.filter((e) => e.kind === "component").map((e) => e.to)
  );
  const driverSources = new Set(tree.edges.filter((e) => e.kind === "driver").map((e) => e.from));

  for (const node of tree.nodes) {
    if (node.is_composite || componentTargets.has(node.id)) {
      roles.set(node.id, "composite");
    } else if (driverSources.has(node.id)) {
      roles.set(node.id, "driver");
    } else if (tree.edges.some((e) => e.to === node.id && e.kind === "component")) {
      roles.set(node.id, "component");
    } else {
      roles.set(node.id, "leaf");
    }
  }
  return roles;
}

export function MetricTreeGraph({ tree, selectedId, onSelect }: MetricTreeGraphProps) {
  const theme = useTheme((s) => s.theme);
  const [hideOrphans, setHideOrphans] = useState(true);

  const filteredTree = useMemo<MetricTree>(() => {
    if (!hideOrphans) return tree;
    const connected = new Set<string>();
    for (const e of tree.edges) {
      connected.add(e.from);
      connected.add(e.to);
    }
    return { ...tree, nodes: tree.nodes.filter((n) => connected.has(n.id)) };
  }, [tree, hideOrphans]);

  const roles = useMemo(() => deriveNodeRoles(filteredTree), [filteredTree]);

  const { nodes: rawNodes, edges } = useMemo(
    () => metricTreeToFlow(filteredTree, selectedId, roles),
    [filteredTree, selectedId, roles]
  );

  const [positioned, setPositioned] = useState<RFNode[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setPositioned(null);
    layoutWithElk(rawNodes, edges)
      .then((laidOut) => {
        if (!cancelled) setPositioned(laidOut);
      })
      .catch((error) => {
        console.error("metric tree layout failed", error);
        if (!cancelled) setPositioned(rawNodes);
      });
    return () => {
      cancelled = true;
    };
  }, [rawNodes, edges]);

  if (tree.nodes.length === 0) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
        No measures found in this workspace's semantic layer.
      </div>
    );
  }

  const orphanCount = tree.nodes.length - filteredTree.nodes.length;

  return (
    <div className='relative h-full w-full'>
      <div className='absolute top-3 right-3 z-10 flex items-center gap-2 rounded-lg border border-border bg-card/90 px-3 py-1.5 shadow-sm backdrop-blur'>
        <Switch
          id='metric-tree-hide-orphans'
          checked={hideOrphans}
          onCheckedChange={setHideOrphans}
        />
        <Label htmlFor='metric-tree-hide-orphans' className='cursor-pointer text-xs'>
          Hide unconnected
          {hideOrphans && orphanCount > 0 && (
            <span className='ml-1 text-muted-foreground'>({orphanCount})</span>
          )}
        </Label>
      </div>

      {filteredTree.nodes.length === 0 ? (
        <div className='flex h-full flex-col items-center justify-center gap-1 text-muted-foreground text-sm'>
          <p>All measures are unconnected.</p>
          <p className='text-xs'>Toggle "Hide unconnected" off to see them.</p>
        </div>
      ) : positioned === null ? (
        <div className='flex h-full items-center justify-center text-muted-foreground text-xs'>
          Laying out…
        </div>
      ) : (
        <ReactFlow
          key={`${filteredTree.nodes.length}-${edges.length}`}
          nodes={positioned}
          edges={edges}
          nodeTypes={nodeTypes}
          colorMode={theme as ColorMode}
          onNodeClick={(_event, node) => onSelect(node.id)}
          fitView
          fitViewOptions={{ padding: 0.15, minZoom: 0.1 }}
          minZoom={0.05}
          proOptions={{ hideAttribution: true }}
        >
          <Background variant={BackgroundVariant.Dots} gap={20} size={1} className='opacity-40' />
          <Controls showInteractive={false} className='rounded-lg border border-border shadow-sm' />
        </ReactFlow>
      )}
    </div>
  );
}
