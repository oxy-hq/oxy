import { useMemo } from "react";
import type { BuilderFileChange } from "@/hooks/useBuilderActivity";
import { GenericGraph } from "./GenericGraph";
import { diffTopicViews, type TopicConfig } from "./types";

// ── Topic graph ───────────────────────────────────────────────────────────────
export const SemanticTopicGraph = ({
  change,
  oldTopic,
  newTopic
}: {
  change: BuilderFileChange;
  oldTopic: TopicConfig | null;
  newTopic: TopicConfig;
}) => {
  const diffs = useMemo(() => diffTopicViews(oldTopic, newTopic), [oldTopic, newTopic]);
  const changedItems = useMemo(() => diffs.filter((d) => d.status !== "unchanged"), [diffs]);
  const viewCount = (newTopic.views ?? []).length;
  return (
    <GenericGraph
      change={change}
      graphLabel='Topic Graph'
      rootLabel='Topic'
      rootTitle={newTopic.name ?? "untitled"}
      rootSubtitle={`${viewCount} views`}
      changedItems={changedItems}
    />
  );
};
