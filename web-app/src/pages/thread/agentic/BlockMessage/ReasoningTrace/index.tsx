import { useCallback, useMemo } from "react";
import useSaveAutomationMutation from "@/hooks/api/workflows/useSaveAutomationMutation";
import useWorkflows from "@/hooks/api/workflows/useWorkflows";
import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import type { RunInfo } from "@/services/types";
import type { GetBlocksResponse } from "@/services/types/runs";
import {
  getMessageReasoningSteps,
  useMessageReasoningSteps,
  useMessageStreaming,
  useSelectedMessageReasoning
} from "@/stores/agentic";
import useTaskThreadStore from "@/stores/useTaskThread";
import type { TaskConfig } from "@/stores/useWorkflow";
import {
  buildStepDagMapping,
  convertReasoningToTasks,
  generateAutomationDescription,
  generateAutomationName
} from "../../convertReasoningToTasks";
import AutomationIndicator from "./AutomationIndicator";
import CollapsedDotSummary from "./CollapsedDotSummary";
import ReasoningTraceHeader from "./Header";
import ReasoningStepRow from "./ReasoningStepRow";
import useAutoCollapse from "./useAutoCollapse";

const EXPANDED_MAX_HEIGHT = "max-h-[600px]";

export interface AutomationGenerated {
  tasks: TaskConfig[];
  path: string;
  runInfo: RunInfo;
}

interface ReasoningTraceProps {
  automationGenerated?: AutomationGenerated;
  runInfo: GetBlocksResponse;
  threadId: string;
  onAutomationGenerated?: (automation: AutomationGenerated) => void;
  onStepHover: (dagNodeId: string | null) => void;
  hoveredNodeId: string | null;
}

function hasRouteWithGroup(steps: Step[]) {
  return steps.some((s) => s.step_type === "route" && s.routeGroupId);
}

const ReasoningTrace = ({
  runInfo,
  threadId,
  automationGenerated,
  hoveredNodeId,
  onAutomationGenerated,
  onStepHover
}: ReasoningTraceProps) => {
  const { selectBlock } = useSelectedMessageReasoning();
  const isStreaming = useMessageStreaming(runInfo);
  const steps = useMessageReasoningSteps(runInfo);
  const { getTaskThread } = useTaskThreadStore();
  const { mutate: triggerSaveAutomation, isPending } = useSaveAutomationMutation();

  const stepDagMap = useMemo(() => buildStepDagMapping(steps), [steps]);

  const { data: existingWorkflows } = useWorkflows();
  const firstHumanContent = useMemo(() => {
    const threadMessages = getTaskThread(threadId).messages;
    return threadMessages.find((m) => m.is_human)?.content;
  }, [getTaskThread, threadId]);

  const existingAutomationName = useMemo(() => {
    if (!existingWorkflows || !firstHumanContent) return undefined;
    const proposedName = generateAutomationName([], firstHumanContent).toLowerCase();
    return existingWorkflows.find((w) => w.name.toLowerCase() === proposedName)?.name;
  }, [existingWorkflows, firstHumanContent]);

  const handleAutomateClick = useCallback(() => {
    const reasoningSteps = getMessageReasoningSteps(runInfo);
    if (reasoningSteps.length === 0 || isPending) return;

    const tasks = convertReasoningToTasks(reasoningSteps);
    const threadMessages = getTaskThread(threadId).messages;
    const firstHumanMessage = threadMessages.find((m) => m.is_human);
    const name = generateAutomationName(reasoningSteps, firstHumanMessage?.content);
    const description = generateAutomationDescription(reasoningSteps);
    const include = firstHumanMessage ? [firstHumanMessage.content] : [description];

    triggerSaveAutomation(
      { name, description, tasks, retrieval: { include, exclude: [] } },
      {
        onSuccess: (data) => {
          onAutomationGenerated?.({
            tasks,
            path: data.path,
            runInfo
          });
        }
      }
    );
  }, [getTaskThread, threadId, runInfo, onAutomationGenerated, triggerSaveAutomation, isPending]);

  const isComplete = !isStreaming && steps.length > 0;
  const canAutomate = !automationGenerated && isComplete && !hasRouteWithGroup(steps);

  const [collapsed, setCollapsed] = useAutoCollapse(isStreaming, steps.length > 0);

  const toggleCollapse = useCallback(
    () => isComplete && setCollapsed((prev) => !prev),
    [isComplete, setCollapsed]
  );

  const onArtifactClick = useCallback(
    (blockId: string) => selectBlock(blockId, runInfo),
    [selectBlock, runInfo]
  );

  const shouldShowAutomate = canAutomate && isComplete;

  if (!isStreaming && steps.length === 0) return null;

  return (
    <div className='space-y-1.5 rounded-lg border border-border bg-card p-3'>
      <ReasoningTraceHeader
        isStreaming={isStreaming}
        steps={steps}
        toggleCollapse={toggleCollapse}
        collapsed={collapsed}
      />

      <div
        className={cn(
          "transition-all duration-500",
          collapsed
            ? "max-h-0 overflow-hidden opacity-0"
            : `${EXPANDED_MAX_HEIGHT} overflow-y-auto opacity-100`
        )}
      >
        <div className='space-y-1.5'>
          {steps.map((step) => (
            <ReasoningStepRow
              key={step.id}
              step={step}
              dagNodeId={stepDagMap.get(step.id) ?? null}
              hoveredNodeId={hoveredNodeId}
              showDagMapping={
                !!automationGenerated?.runInfo.lookup_id &&
                automationGenerated.runInfo.lookup_id === runInfo.lookup_id
              }
              onStepHover={onStepHover}
              onArtifactClick={onArtifactClick}
            />
          ))}
        </div>

        {shouldShowAutomate && (
          <div className='fade-in slide-in-from-bottom-2 mt-3 animate-in duration-500'>
            <AutomationIndicator
              steps={steps}
              dagLinkedCount={stepDagMap.size}
              onGenerate={handleAutomateClick}
              existingAutomationName={existingAutomationName}
              isLoading={isPending}
            />
          </div>
        )}
      </div>

      {collapsed && isComplete && (
        <CollapsedDotSummary
          steps={steps}
          canAutomate={canAutomate}
          onAutomateClick={handleAutomateClick}
          isLoading={isPending}
        />
      )}
    </div>
  );
};

export default ReasoningTrace;
