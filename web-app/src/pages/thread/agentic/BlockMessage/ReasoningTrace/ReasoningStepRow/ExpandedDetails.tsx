import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import type { Block } from "@/services/types";
import { STEP_COLOR_BORDER, STEP_COLOR_DOT, STEP_COLOR_TEXT } from "../colors";
import ArtifactPill from "./ArtifactPill";
import { ARTIFACT_TYPES, getArtifactLabel, stripUuids } from "./helpers";

const ChildBlockList = ({
  blocks,
  onArtifactClick
}: {
  blocks: Block[];
  onArtifactClick?: (blockId: string) => void;
}) => (
  <div className='mt-1.5 space-y-1'>
    {blocks.map((child) => {
      if (ARTIFACT_TYPES.has(child.type) && onArtifactClick) {
        return (
          <ArtifactPill
            key={child.id}
            block={child}
            label={getArtifactLabel(child)}
            onClick={() => onArtifactClick(child.id)}
          />
        );
      }
      return (
        <p key={child.id} className='line-clamp-3 text-xs'>
          {child.type === "text" ? child.content : child.type}
        </p>
      );
    })}
  </div>
);

const DagMappingLabel = ({ stepType, dagNodeId }: { stepType: string; dagNodeId: string }) => (
  <div className='mt-1.5 flex items-center gap-1.5'>
    <div className={cn("h-1.5 w-1.5 rounded-full", STEP_COLOR_DOT[stepType])} />
    <span className={cn("font-mono text-xs", STEP_COLOR_TEXT[stepType])}>maps to {dagNodeId}</span>
  </div>
);

interface ExpandedDetailsProps {
  step: Step;
  expanded: boolean;
  dagNodeId: string | null;
  showDagMapping: boolean;
  onArtifactClick?: (blockId: string) => void;
}

const ExpandedDetails = ({
  step,
  expanded,
  dagNodeId,
  showDagMapping,
  onArtifactClick
}: ExpandedDetailsProps) => {
  const stepType = step.step_type;
  const isDone = !step.is_streaming && !step.error;

  return (
    <div
      className={cn(
        "overflow-hidden transition-all duration-300",
        expanded ? "max-h-40 opacity-100" : "max-h-0 opacity-0"
      )}
    >
      <div
        className={cn(
          "mt-1 mb-2 ml-8 border-l-2 pl-3 text-muted-foreground text-sm leading-relaxed",
          STEP_COLOR_BORDER[stepType]
        )}
      >
        {step.objective && <p>{stripUuids(step.objective)}</p>}
        {step.error && <p className='mt-1 text-destructive'>{step.error}</p>}

        {step.childrenBlocks.length > 0 && (
          <ChildBlockList blocks={step.childrenBlocks} onArtifactClick={onArtifactClick} />
        )}

        {showDagMapping && dagNodeId && isDone && (
          <DagMappingLabel stepType={stepType} dagNodeId={dagNodeId} />
        )}
      </div>
    </div>
  );
};

export default ExpandedDetails;
