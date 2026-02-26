import { ChevronRight } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import {
  STEP_COLOR_BG,
  STEP_COLOR_BORDER,
  STEP_COLOR_DOT,
  STEP_COLOR_TEXT,
  STEP_ICON
} from "../colors";
import ArtifactPill from "./ArtifactPill";
import { findArtifactBlock, stripUuids } from "./helpers";
import RoutePill from "./RoutePill";
import StatusBadge from "./StatusBadge";

function getRowStyle(isHighlighted: boolean, isRunning: boolean, stepType: string): string {
  if (isHighlighted) {
    return cn("border-l-2", STEP_COLOR_BORDER[stepType], STEP_COLOR_BG[stepType]);
  }
  if (isRunning) {
    return cn("border-l-2", STEP_COLOR_BORDER[stepType], "bg-secondary/80");
  }
  return "border-transparent bg-secondary/50 hover:bg-secondary";
}

interface StepHeaderProps {
  step: Step;
  dagNodeId: string | null;
  hoveredNodeId: string | null;
  expanded: boolean;
  onToggle: () => void;
  onStepHover: (dagNodeId: string | null) => void;
  onArtifactClick?: (blockId: string) => void;
}

const StepHeader = ({
  step,
  dagNodeId,
  hoveredNodeId,
  expanded,
  onToggle,
  onStepHover,
  onArtifactClick
}: StepHeaderProps) => {
  const stepType = step.step_type;
  const Icon = STEP_ICON[stepType] ?? STEP_ICON.idle;

  const isRunning = !!step.is_streaming;
  const hasError = !!step.error;
  const isDone = !isRunning && !hasError;
  const isHighlighted = dagNodeId !== null && dagNodeId === hoveredNodeId;
  const isActive = isHighlighted || isRunning;
  const hasDagLink = !!dagNodeId;

  const artifactBlock = isDone ? findArtifactBlock(step.childrenBlocks) : null;
  const displayText = step.objective ? stripUuids(step.objective) : stepType;
  const routeName = stepType === "route" ? step.routeName : undefined;

  return (
    <div
      role='button'
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onToggle();
        }
      }}
      onMouseEnter={() => onStepHover(dagNodeId)}
      onMouseLeave={() => onStepHover(null)}
      className='group w-full cursor-pointer text-left'
    >
      <div
        className={cn(
          "flex items-center gap-2 rounded-md border px-3 py-1.5 transition-all duration-200",
          getRowStyle(isHighlighted, isRunning, stepType)
        )}
      >
        <div
          className={cn(
            "h-1.5 w-1.5 shrink-0 rounded-full transition-all duration-200",
            STEP_COLOR_DOT[stepType],
            isHighlighted && "scale-150",
            isRunning && "animate-pulse",
            !hasDagLink && !isRunning && "opacity-30"
          )}
        />

        <Icon
          className={cn(
            "h-3.5 w-3.5 shrink-0 transition-colors duration-200",
            isActive ? STEP_COLOR_TEXT[stepType] : "text-muted-foreground"
          )}
        />

        <span
          className={cn(
            "flex-1 truncate text-sm transition-colors duration-200",
            isActive ? "text-foreground" : "text-muted-foreground"
          )}
        >
          {displayText}
        </span>

        <StatusBadge isRunning={isRunning} isDone={isDone} hasError={hasError} />

        {artifactBlock && dagNodeId && onArtifactClick && (
          <ArtifactPill
            block={artifactBlock}
            label={dagNodeId}
            onClick={() => onArtifactClick(artifactBlock.id)}
          />
        )}

        {routeName && step.routeGroupId && onArtifactClick && (
          <RoutePill
            name={routeName}
            // biome-ignore lint/style/noNonNullAssertion: <already checked for existence>
            onClick={() => onArtifactClick(step.routeGroupId!)}
          />
        )}

        <ChevronRight
          className={cn(
            "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
            expanded && "rotate-90"
          )}
        />
      </div>
    </div>
  );
};

export default StepHeader;
