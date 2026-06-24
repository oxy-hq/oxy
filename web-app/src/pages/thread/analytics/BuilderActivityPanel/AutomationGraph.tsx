import { useMemo } from "react";
import type { BuilderFileChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { type AutomationConfig, automationKind, diffAutomationTasks } from "./types";

// ── Automation graph ────────────────────────────────────────────────────────────
export const AutomationGraph = ({
  change,
  oldWf,
  newWf
}: {
  change: BuilderFileChange;
  oldWf: AutomationConfig | null;
  newWf: AutomationConfig;
}) => {
  const diffs = useMemo(() => diffAutomationTasks(oldWf, newWf), [oldWf, newWf]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  // If no task-level changes were detected (e.g. only automation metadata changed),
  // fall back to showing all tasks so the graph is never empty.
  const displayItems = changedItems.length > 0 ? changedItems : diffs;
  const kind = automationKind(change.filePath);
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
