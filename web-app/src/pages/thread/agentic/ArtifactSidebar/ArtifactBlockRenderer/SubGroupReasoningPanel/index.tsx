import { ExternalLink, GitBranch, LoaderCircle } from "lucide-react";
import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { encodeBase64 } from "@/libs/encoding";
import ROUTES from "@/libs/utils/routes";
import Reasoning from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import {
  useGroupReasoningSteps,
  useGroupStreaming,
  useSelectedMessageReasoning
} from "@/stores/agentic";
import { useBlockStore } from "@/stores/block";

const SubGroupReasoningPanel = ({ groupId }: { groupId: string }) => {
  const { setSelectedBlockId } = useSelectedMessageReasoning();
  const { project } = useCurrentProjectBranch();
  const navigate = useNavigate();
  const isStreaming = useGroupStreaming(groupId);
  const groups = useBlockStore((state) => state.groups);
  const groupBlocks = useBlockStore((state) => state.groupBlocks);
  const steps = useGroupReasoningSteps(groupId);

  const sourcePath = useMemo(() => {
    const group = groups[groupId];
    if (!group) return undefined;
    if (group.type === "artifact" && group.artifact_metadata?.type === "workflow") {
      return group.artifact_metadata.workflow_id;
    }
    if (group.type === "workflow") {
      return group.workflow_id;
    }
    const gb = groupBlocks[groupId];
    if (gb) {
      for (const block of Object.values(gb.blocks)) {
        if (block.type === "group") {
          const nested = groups[block.group_id];
          if (nested?.type === "workflow") return nested.workflow_id;
        }
      }
    }
    return undefined;
  }, [groupId, groups, groupBlocks]);

  if (steps.length === 0 && isStreaming) {
    return (
      <div className='flex h-full items-center justify-center'>
        <LoaderCircle className='h-6 w-6 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (steps.length === 0) {
    return (
      <div className='flex h-full flex-col items-center justify-center gap-3 p-6 text-center'>
        <div className='flex h-10 w-10 items-center justify-center rounded-full bg-node-plan/10'>
          <GitBranch className='h-5 w-5 text-node-plan' />
        </div>
        <p className='text-muted-foreground text-sm'>No trace data available</p>
      </div>
    );
  }

  const header = sourcePath ? (
    <div className='mb-2 flex items-center justify-end'>
      <button
        type='button'
        onClick={() => navigate(ROUTES.PROJECT(project.id).WORKFLOW(encodeBase64(sourcePath)).ROOT)}
        className='flex items-center gap-1 text-muted-foreground text-xs transition-colors hover:text-foreground'
      >
        <ExternalLink className='h-3 w-3' />
        <span>Open in editor</span>
      </button>
    </div>
  ) : null;

  return (
    <Reasoning
      steps={steps}
      onFullscreen={(blockId) => setSelectedBlockId(blockId)}
      header={header}
    />
  );
};

export default SubGroupReasoningPanel;
