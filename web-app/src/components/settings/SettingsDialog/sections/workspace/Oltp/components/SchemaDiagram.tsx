import {
  Background,
  BackgroundVariant,
  Controls,
  type Edge,
  Handle,
  MarkerType,
  type Node,
  type NodeProps,
  Position,
  ReactFlow,
  ReactFlowProvider
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { KeyRound } from "lucide-react";
import { useMemo } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import useOltpErd from "@/hooks/api/oltp/useOltpErd";
import { cn } from "@/libs/shadcn/utils";
import type { ErdResponse, ErdSchema, ErdTable } from "@/services/api/oltp";

interface TableNodeData extends Record<string, unknown> {
  table: ErdTable;
  schema: string;
  kind: ErdSchema["kind"];
}

/** One table: header plus its columns, primary keys marked. */
function TableNode({ data }: NodeProps<Node<TableNodeData>>) {
  const { table, schema, kind } = data;
  return (
    <div
      className={cn(
        "w-56 overflow-hidden rounded-lg border bg-card shadow-sm",
        // The app/pipeline distinction is the point of the diagram, so it reads
        // in the border as well as the schema heading.
        kind === "app" ? "border-primary/40" : "border-border"
      )}
    >
      <Handle type='target' position={Position.Left} className='!size-1.5 !border-0 !bg-border' />
      <div className='border-b bg-muted/40 px-2.5 py-1.5'>
        <div className='truncate font-medium text-sm'>{table.name}</div>
        <div className='truncate font-mono text-muted-foreground text-xs'>{schema}</div>
      </div>
      <div className='divide-y'>
        {table.columns.map((c) => (
          <div key={c.name} className='flex items-center gap-1.5 px-2.5 py-1'>
            {c.is_primary_key ? (
              <KeyRound className='size-3 shrink-0 text-primary' />
            ) : (
              <span className='size-3 shrink-0' />
            )}
            <span className='truncate font-mono text-xs'>{c.name}</span>
            <span className='ml-auto shrink-0 text-muted-foreground text-xs'>{c.data_type}</span>
          </div>
        ))}
      </div>
      <Handle type='source' position={Position.Right} className='!size-1.5 !border-0 !bg-border' />
    </div>
  );
}

const nodeTypes = { erdTable: TableNode };

const NODE_WIDTH = 224; // w-56
const COLUMN_GAP = 120;
const ROW_GAP = 40;
const HEADER_ROOM = 32;

/**
 * Deterministic layout: one column per schema, tables stacked within it.
 *
 * A force/elk layout would scatter tables by connectivity, which buries the
 * thing this diagram exists to show — that each writer owns a schema. Grouping
 * by schema is the information.
 */
function buildGraph(erd: ErdResponse): { nodes: Node<TableNodeData>[]; edges: Edge[] } {
  const nodes: Node<TableNodeData>[] = [];

  erd.schemas.forEach((schema, col) => {
    let y = HEADER_ROOM;
    schema.tables.forEach((table) => {
      nodes.push({
        id: `${schema.name}.${table.name}`,
        type: "erdTable",
        position: { x: col * (NODE_WIDTH + COLUMN_GAP), y },
        data: { table, schema: schema.name, kind: schema.kind }
      });
      // Header + one row per column, so stacked tables never overlap.
      y += 52 + table.columns.length * 26 + ROW_GAP;
    });
  });

  const edges: Edge[] = erd.relationships.map((r) => ({
    id: `${r.from_schema}.${r.from_table}.${r.from_column}->${r.to_schema}.${r.to_table}.${r.to_column}`,
    source: `${r.from_schema}.${r.from_table}`,
    target: `${r.to_schema}.${r.to_table}`,
    label: `${r.from_column} → ${r.to_column}`,
    markerEnd: { type: MarkerType.ArrowClosed },
    style: { strokeWidth: 1.5 }
  }));

  return { nodes, edges };
}

const SchemaDiagram: React.FC<{ workspaceId: string | undefined }> = ({ workspaceId }) => {
  const { data: erd, isLoading, error } = useOltpErd(workspaceId);
  const graph = useMemo(() => (erd ? buildGraph(erd) : { nodes: [], edges: [] }), [erd]);

  if (isLoading) {
    return <p className='text-muted-foreground text-sm'>Loading diagram…</p>;
  }
  if (error) {
    return (
      <p className='text-muted-foreground text-sm'>
        Couldn't load the diagram: {error instanceof Error ? error.message : "unknown error"}
      </p>
    );
  }
  if (!erd || graph.nodes.length === 0) {
    return (
      <p className='text-muted-foreground text-sm'>
        No tables yet. Each app or pipeline creates its own the first time it writes.
      </p>
    );
  }

  return (
    <div className='flex flex-col gap-2'>
      <div className='flex flex-wrap items-center gap-2'>
        {erd.schemas.map((s) => (
          <Badge key={s.name} variant={s.kind === "app" ? "secondary" : "outline"}>
            <span className='font-mono'>{s.name}</span>
            <span className='ml-1 opacity-70'>
              {s.kind === "app" ? "app" : s.kind === "pipeline" ? "pipeline" : "unowned"}
            </span>
          </Badge>
        ))}
      </div>
      <div className='h-96 overflow-hidden rounded-md border'>
        <ReactFlowProvider>
          <ReactFlow
            nodes={graph.nodes}
            edges={graph.edges}
            nodeTypes={nodeTypes}
            fitView
            minZoom={0.2}
            proOptions={{ hideAttribution: true }}
            nodesDraggable
            nodesConnectable={false}
            edgesFocusable={false}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
            <Controls showInteractive={false} />
          </ReactFlow>
        </ReactFlowProvider>
      </div>
      <p className='text-muted-foreground text-xs'>
        Structure only, read as <code className='font-mono'>{erd.read_as_role}</code>. Rows are
        never fetched here, and nothing on this screen can change the database.
      </p>
    </div>
  );
};

export default SchemaDiagram;
