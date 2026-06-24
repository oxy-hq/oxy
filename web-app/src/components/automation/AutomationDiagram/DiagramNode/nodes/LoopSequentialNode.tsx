import { useMemo } from "react";
import { useSelectLoop, useStepStatus } from "@/pages/automation/components/RunStatusContext";
import useAutomation, {
  type LoopSequentialTaskConfig,
  type TaskConfigWithId
} from "@/stores/useAutomation";
import {
  distanceBetweenHeaderAndContent,
  headerHeight,
  loopProgressBarHeight,
  nodeBorderHeight,
  paddingHeight
} from "../../layout/constants";
import { LoopProgressBar } from "./LoopProgressBar";
import { NodeHeader } from "./NodeHeader";

type Props = {
  parentId?: string;
  task: TaskConfigWithId;
  expanded?: boolean;
};

/**
 * Loop-iteration node header + spacer + live progress bar.
 *
 * The legacy "loop value" combobox (per-iteration picker driven by
 * the old block-store data) was removed when the automation runtime
 * moved to agentic-automations. The bar shown here is its replacement:
 * always-visible during execution, no click required, no dropdown.
 * Per-iteration drill-down lives in the Retry popover's
 * `IterationGrid` (post-completion) for now; "open live grid in
 * sidebar" is the natural click-target follow-up.
 *
 * `parentId` is still accepted because the diagram's layout code
 * passes it; it's unused by this view.
 */
export function LoopSequentialNode({ task, expanded }: Props) {
  const nodes = useAutomation((state) => state.nodes);
  const setNodeExpanded = useAutomation((state) => state.setNodeExpanded);
  const loop = task as LoopSequentialTaskConfig;
  const tasks = loop.tasks;
  const expandable = useMemo(() => tasks.length > 0, [tasks]);

  // Live iteration state — `useStepStatus` returns undefined when no
  // run-status provider is mounted (e.g., the legacy preview path)
  // or when the loop hasn't fanned out yet. Both cases collapse to
  // the bar rendering nothing, which is what we want.
  const live = useStepStatus(task.name);
  const iterations = live?.iterations ?? [];

  // Page-level click handler — `null` on legacy paths where no
  // sidebar Iterations tab is mounted. When null, LoopProgressBar
  // skips the click affordance entirely (cursor stays default).
  const selectLoop = useSelectLoop();
  const onBarClick = selectLoop ? () => selectLoop(task.name) : undefined;

  // Static total when the YAML lists values inline (`values: [a, b, c]`).
  // For Jinja-expression `values:` strings, we can't know the total
  // until the decider resolves it at runtime, so we fall back to
  // whatever's been observed in the event stream so far.
  const staticTotal = useMemo(
    () => (Array.isArray(loop.values) ? loop.values.length : undefined),
    [loop.values]
  );

  const node = nodes.find((n) => n.id === task.id);
  const onExpandClick = () => {
    setNodeExpanded(task.id, !expanded);
  };
  if (!node?.height) return null;
  // `usedHeight` must mirror the parent-height accounting in
  // `nodeSize.ts::computeVerticalContainerSize` + the ELK top
  // padding in `elkLayout.ts::calculateNodePadding`. The two
  // `distanceBetweenHeaderAndContent` terms are the StepContainer
  // `gap-2`s: one between header and bar slot, one between bar
  // slot and the children-row.
  const usedHeight =
    headerHeight +
    distanceBetweenHeaderAndContent +
    loopProgressBarHeight +
    distanceBetweenHeaderAndContent +
    paddingHeight +
    nodeBorderHeight;
  const childSpace = node.height - usedHeight;
  return (
    <>
      <NodeHeader
        name={task.name}
        type={task.type}
        expandable={expandable}
        expanded={expanded}
        onExpandClick={onExpandClick}
      />
      {/* Bar always occupies its reserved slot so the parent node's
          height stays stable across "loop hasn't fanned out yet"
          and "loop is running" — the slot is included in
          calculateContainerDimensions / calculateNodePadding for
          LOOP_SEQUENTIAL nodes regardless of runtime state.
          When there's no data + no static total, the bar component
          returns null but the slot's reserved space stays. */}
      <div style={{ height: `${loopProgressBarHeight}px` }} className='flex items-center'>
        <LoopProgressBar iterations={iterations} total={staticTotal} onClick={onBarClick} />
      </div>
      {expandable && expanded && <div style={{ height: `${childSpace}px` }} />}
    </>
  );
}
