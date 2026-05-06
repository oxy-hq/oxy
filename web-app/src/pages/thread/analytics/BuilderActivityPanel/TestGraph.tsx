import { useMemo } from "react";

import type { BuilderFileChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { diffTestCases, type TestFileConfig } from "./types";

export const TestGraph = ({
  change,
  oldTest,
  newTest
}: {
  change: BuilderFileChange;
  oldTest: TestFileConfig | null;
  newTest: TestFileConfig;
}) => {
  const diffs = useMemo(() => diffTestCases(oldTest, newTest), [oldTest, newTest]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  // If no case-level changes were detected, show all cases so the graph is never empty.
  const displayItems = changedItems.length > 0 ? changedItems : diffs;

  const caseCount = (newTest.cases ?? []).length;
  const target = newTest.target;

  return (
    <GenericGraph
      change={change}
      graphLabel='Test Graph'
      rootLabel='Test'
      rootTitle={newTest.name ?? target ?? "untitled"}
      rootSubtitle={`${caseCount} cases${target ? ` · ${target}` : ""}`}
      changedItems={displayItems}
    />
  );
};
