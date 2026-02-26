import { Check, Loader2, Workflow, Zap } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";

interface AutomationIndicatorProps {
  steps: Step[];
  dagLinkedCount: number;
  onGenerate: () => void;
  existingAutomationName?: string;
  isLoading?: boolean;
}

const AutomationIndicator = ({
  steps,
  dagLinkedCount,
  onGenerate,
  existingAutomationName,
  isLoading = false
}: AutomationIndicatorProps) => {
  const querySteps = steps.filter(
    (s) => s.step_type === "semantic_query" || s.step_type === "query"
  );
  const hasSemanticQuery = steps.some((s) => s.step_type === "semantic_query");
  const hasMultipleQueries = querySteps.length >= 2;
  const isGoodCandidate = hasSemanticQuery || hasMultipleQueries;
  const total = steps.length;

  if (existingAutomationName) {
    return (
      <div className='flex items-center gap-3 rounded-md border border-border bg-secondary/30 px-3 py-2 transition-all duration-500'>
        <Check className='h-3.5 w-3.5 shrink-0 text-primary' />
        <div className='min-w-0 flex-1'>
          <div className='font-medium text-foreground text-sm leading-tight'>
            Similar automation exists
          </div>
          <div className='mt-0.5 text-muted-foreground text-xs'>{existingAutomationName}</div>
        </div>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-md border px-3 py-2 transition-all duration-500",
        isGoodCandidate ? "border-primary/30 bg-primary/5" : "border-border bg-secondary/30"
      )}
    >
      <Zap
        className={cn(
          "h-3.5 w-3.5 shrink-0",
          isGoodCandidate ? "text-primary" : "text-muted-foreground"
        )}
      />
      <div className='min-w-0 flex-1'>
        <div className='font-medium text-foreground text-sm leading-tight'>
          {isGoodCandidate ? "Good candidate for automation" : "Low automation potential"}
        </div>
        <div className='mt-0.5 font-mono text-muted-foreground text-xs'>
          {querySteps.length} {querySteps.length === 1 ? "query" : "queries"} &middot;{" "}
          {dagLinkedCount}/{total} steps mappable
        </div>
      </div>
      <Button
        size='sm'
        variant={isGoodCandidate ? "default" : "secondary"}
        onClick={onGenerate}
        disabled={isLoading}
      >
        {isLoading ? (
          <Loader2 className='h-3 w-3 animate-spin' />
        ) : (
          <Workflow className='h-3 w-3' />
        )}
        <span>{isLoading ? "Saving…" : "Automate this"}</span>
      </Button>
    </div>
  );
};

export default AutomationIndicator;
