import { useMemo } from "react";
import type { BuilderFileChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { diffWorkflowTasks, type WorkflowConfig, workflowKind } from "./types";

// ── Workflow graph ────────────────────────────────────────────────────────────
export const WorkflowGraph = ({
  change,
  oldWf,
  newWf
}: {
  change: BuilderFileChange;
  oldWf: WorkflowConfig | null;
  newWf: WorkflowConfig;
}) => {
  const diffs = useMemo(() => diffWorkflowTasks(oldWf, newWf), [oldWf, newWf]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  // If no task-level changes were detected (e.g. only workflow metadata changed),
  // fall back to showing all tasks so the graph is never empty.
  const displayItems = changedItems.length > 0 ? changedItems : diffs;
  const kind = workflowKind(change.filePath);
  const taskCount = (newWf.tasks ?? []).length;
  return (
    <GenericGraph
      change={change}
      graphLabel={`${kind} Graph`}
      rootLabel={kind}
      rootTitle={newWf.name ?? "untitled"}
      rootSubtitle={`${taskCount} tasks`}
      changedItems={displayItems}
    />
  );
};
