import { useMemo } from "react";
import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { type DataApp, diffAppDisplays, diffAppTasks } from "./types";

// ── Per-type graph wrappers ───────────────────────────────────────────────────
export const DataAppGraph = ({
  change,
  oldApp,
  newApp
}: {
  change: BuilderProposedChange;
  oldApp: DataApp | null;
  newApp: DataApp;
}) => {
  const taskDiffs = useMemo(() => diffAppTasks(oldApp, newApp), [oldApp, newApp]);
  const displayDiffs = useMemo(() => diffAppDisplays(oldApp, newApp), [oldApp, newApp]);
  const changedTasks = useMemo(
    () => taskDiffs.filter((d) => d.status !== "unchanged"),
    [taskDiffs]
  );
  const changedDisplays = useMemo(
    () => displayDiffs.filter((d) => d.status !== "unchanged"),
    [displayDiffs]
  );
  const taskCount = (newApp.tasks ?? []).length;
  const displayCount = (newApp.display ?? []).length;
  return (
    <GenericGraph
      change={change}
      graphLabel='App Graph'
      rootLabel='App'
      rootTitle={newApp.name ?? "untitled"}
      rootSubtitle={`${taskCount} tasks · ${displayCount} displays`}
      changedItems={[...changedTasks, ...changedDisplays]}
      itemGroups={[
        { label: "Tasks", items: changedTasks },
        { label: "Displays", items: changedDisplays }
      ]}
    />
  );
};
