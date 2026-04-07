import { useMemo } from "react";

import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { type AwConfig, diffAwTransitions } from "./types";

export const AwGraph = ({
  change,
  oldAw,
  newAw
}: {
  change: BuilderProposedChange;
  oldAw: AwConfig | null;
  newAw: AwConfig;
}) => {
  const diffs = useMemo(() => diffAwTransitions(oldAw, newAw), [oldAw, newAw]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  const transCount = (newAw.transitions ?? []).length;
  const subtitle = [`${transCount} transitions`, newAw.model].filter(Boolean).join(" · ");
  return (
    <GenericGraph
      change={change}
      graphLabel='Agentic Workflow Graph'
      rootLabel='Agentic Workflow'
      rootTitle={newAw.start?.mode ?? "workflow"}
      rootSubtitle={subtitle}
      changedItems={changedItems}
    />
  );
};
