import { Loader2, Zap } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";
import { STEP_COLOR_DOT } from "./colors";

function CollapsedDotSummary({
  steps,
  canAutomate,
  onAutomateClick,
  isLoading = false
}: {
  steps: Step[];
  canAutomate: boolean;
  onAutomateClick: () => void;
  isLoading?: boolean;
}) {
  return (
    <div className='fade-in flex animate-in items-center gap-3 duration-300'>
      <div className='flex items-center gap-1'>
        {steps.map((step) => (
          <div
            key={step.id}
            className={cn(
              "h-1.5 w-1.5 rounded-full",
              STEP_COLOR_DOT[step.step_type] ?? "bg-muted-foreground"
            )}
          />
        ))}
      </div>

      <div className='flex-1' />

      {canAutomate && (
        <button
          type='button'
          onClick={onAutomateClick}
          disabled={isLoading}
          className='flex items-center gap-1.5 font-medium text-primary text-xs transition-colors hover:text-primary/80 disabled:opacity-50'
        >
          {isLoading ? <Loader2 className='h-3 w-3 animate-spin' /> : <Zap className='h-3 w-3' />}
          <span>{isLoading ? "Saving…" : "Automate this"}</span>
        </button>
      )}
    </div>
  );
}

export default CollapsedDotSummary;
