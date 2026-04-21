import {
  ChevronDown,
  ChevronRight,
  Circle,
  GitBranch,
  RefreshCw,
  Square,
  XCircle
} from "lucide-react";
import type React from "react";
import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Spinner } from "@/components/ui/shadcn/spinner";
import useRunTree from "@/hooks/api/coordinator/useRunTree";
import { cn } from "@/libs/shadcn/utils";
import type { TaskTreeNode } from "@/services/api/coordinator";

// ── Tree construction ───────────────────────────────────────────────────────

interface TreeNode extends TaskTreeNode {
  children: TreeNode[];
  depth: number;
}

function buildTree(nodes: TaskTreeNode[], rootId: string): TreeNode | null {
  const byId = new Map<string, TaskTreeNode>();
  for (const n of nodes) byId.set(n.run_id, n);

  const childrenMap = new Map<string, TaskTreeNode[]>();
  for (const n of nodes) {
    if (n.parent_run_id) {
      const siblings = childrenMap.get(n.parent_run_id) ?? [];
      siblings.push(n);
      childrenMap.set(n.parent_run_id, siblings);
    }
  }

  function recurse(id: string, depth: number): TreeNode | null {
    const node = byId.get(id);
    if (!node) return null;
    const kids = (childrenMap.get(id) ?? [])
      .sort((a, b) => a.created_at.localeCompare(b.created_at))
      .map((c) => recurse(c.run_id, depth + 1))
      .filter(Boolean) as TreeNode[];
    return { ...node, children: kids, depth };
  }

  return recurse(rootId, 0);
}

// ── Status helpers ──────────────────────────────────────────────────────────

const statusConfig: Record<string, { label: string; className: string; icon: React.ElementType }> =
  {
    running: { label: "Running", className: "text-primary", icon: RefreshCw },
    suspended: { label: "Suspended", className: "text-warning", icon: Circle },
    done: { label: "Done", className: "text-emerald-500", icon: Circle },
    failed: { label: "Failed", className: "text-destructive", icon: XCircle },
    cancelled: { label: "Cancelled", className: "text-muted-foreground", icon: Square }
  };

const StatusBadge: React.FC<{ status: string }> = ({ status }) => {
  const config = statusConfig[status] ?? statusConfig.running;
  const Icon = config.icon;
  return (
    <span className={cn("inline-flex items-center gap-1 font-medium text-xs", config.className)}>
      <Icon className={cn("h-3 w-3", status === "running" && "animate-spin")} />
      {config.label}
    </span>
  );
};

// ── Duration helper ─────────────────────────────────────────────────────────

function formatDuration(created: string, updated: string): string {
  const ms = new Date(updated).getTime() - new Date(created).getTime();
  const secs = Math.floor(ms / 1000);
  if (secs < 60) return `${secs}s`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ${secs % 60}s`;
  return `${Math.floor(mins / 60)}h ${mins % 60}m`;
}

// ── Tree node component ─────────────────────────────────────────────────────

const TreeNodeRow: React.FC<{ node: TreeNode }> = ({ node }) => {
  const [expanded, setExpanded] = useState(true);
  const hasChildren = node.children.length > 0;

  return (
    <div>
      {/* Node row */}
      <div
        className={cn(
          "flex items-center gap-2 border-border border-b px-3 py-2 hover:bg-muted/50",
          node.depth === 0 && "bg-muted/30"
        )}
        style={{ paddingLeft: `${node.depth * 24 + 12}px` }}
      >
        {/* Expand/collapse */}
        <button
          type='button'
          onClick={() => setExpanded(!expanded)}
          className={cn(
            "flex h-5 w-5 shrink-0 items-center justify-center rounded",
            !hasChildren && "invisible"
          )}
        >
          {expanded ? (
            <ChevronDown className='h-3.5 w-3.5 text-muted-foreground' />
          ) : (
            <ChevronRight className='h-3.5 w-3.5 text-muted-foreground' />
          )}
        </button>

        {/* Branch icon for children */}
        {node.depth > 0 && <GitBranch className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />}

        {/* Status */}
        <div className='w-24 shrink-0'>
          <StatusBadge status={node.status} />
        </div>

        {/* Question / label */}
        <div className='min-w-0 flex-1'>
          <p className='truncate text-sm'>{node.question}</p>
          <div className='mt-0.5 flex items-center gap-2'>
            {node.agent_id && (
              <span className='text-muted-foreground text-xs'>{node.agent_id}</span>
            )}
            {node.source_type && !node.agent_id && (
              <span className='text-muted-foreground text-xs'>{node.source_type}</span>
            )}
            {node.attempt > 0 && (
              <span className='text-warning text-xs'>attempt {node.attempt + 1}</span>
            )}
            {node.outcome_status && (
              <span
                className={cn(
                  "text-xs",
                  node.outcome_status === "done"
                    ? "text-emerald-500"
                    : node.outcome_status === "failed"
                      ? "text-destructive"
                      : "text-muted-foreground"
                )}
              >
                outcome: {node.outcome_status}
              </span>
            )}
          </div>
        </div>

        {/* Duration */}
        <div className='shrink-0 text-right'>
          <span className='text-muted-foreground text-xs'>
            {formatDuration(node.created_at, node.updated_at)}
          </span>
          <div className='font-mono text-muted-foreground text-xs'>{node.run_id.slice(0, 8)}</div>
        </div>
      </div>

      {/* Detail panel: show answer/error when leaf or done/failed */}
      {expanded && (node.answer || node.error_message) && (
        <div
          className='border-border border-b bg-muted/20 px-3 py-2'
          style={{ paddingLeft: `${node.depth * 24 + 48}px` }}
        >
          {node.error_message && (
            <div className='mb-1'>
              <span className='font-medium text-destructive text-xs'>Error: </span>
              <span className='text-xs'>{node.error_message}</span>
            </div>
          )}
          {node.answer && (
            <div>
              <span className='font-medium text-muted-foreground text-xs'>Answer: </span>
              <span className='text-xs'>
                {node.answer.length > 300 ? `${node.answer.slice(0, 300)}...` : node.answer}
              </span>
            </div>
          )}
        </div>
      )}

      {/* Children */}
      {expanded && node.children.map((child) => <TreeNodeRow key={child.run_id} node={child} />)}
    </div>
  );
};

// ── Page component ──────────────────────────────────────────────────────────

const RunTreePage: React.FC = () => {
  const { runId } = useParams<{ runId: string }>();
  const { data, isPending, error, refetch } = useRunTree(runId);

  const tree = useMemo(() => {
    if (!data) return null;
    return buildTree(data.nodes, data.root_id);
  }, [data]);

  if (isPending) {
    return (
      <div className='flex h-full items-center justify-center'>
        <Spinner className='h-6 w-6' />
      </div>
    );
  }

  if (error) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-2'>
        <p className='text-destructive text-sm'>Failed to load task tree</p>
        <Button variant='outline' size='sm' onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  if (!tree) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        <p className='text-sm'>Run not found</p>
      </div>
    );
  }

  const totalNodes = data?.nodes.length ?? 0;

  return (
    <div className='flex h-full flex-col'>
      <div className='flex items-center justify-between border-border border-b px-4 py-3'>
        <div>
          <h2 className='font-semibold text-base'>Task Tree</h2>
          <p className='text-muted-foreground text-xs'>
            {totalNodes} {totalNodes === 1 ? "node" : "nodes"} — root: {runId?.slice(0, 8)}
          </p>
        </div>
        <Button variant='ghost' size='icon' onClick={() => refetch()} className='h-8 w-8'>
          <RefreshCw className='h-4 w-4' />
        </Button>
      </div>

      <div className='flex-1 overflow-y-auto'>
        <TreeNodeRow node={tree} />
      </div>
    </div>
  );
};

export default RunTreePage;
