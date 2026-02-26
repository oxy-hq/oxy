import { ChevronDown, Loader2 } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { Step } from "@/pages/thread/agentic/ArtifactSidebar/ArtifactBlockRenderer/SubGroupReasoningPanel/Reasoning";

interface Props {
  steps: Step[];
  isStreaming: boolean;
  toggleCollapse: () => void;
  collapsed: boolean;
}

function countCompleted(steps: Step[]) {
  return steps.filter((s) => !s.is_streaming && !s.error).length;
}

function formatProgress(steps: Step[], isComplete: boolean) {
  if (isComplete) return `${steps.length} steps completed`;
  return `${countCompleted(steps)}/${steps.length}`;
}

function TraceHeaderIcon({ isComplete, collapsed }: { isComplete: boolean; collapsed: boolean }) {
  if (!isComplete) {
    return <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />;
  }
  return (
    <ChevronDown
      className={cn(
        "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
        collapsed && "-rotate-90"
      )}
    />
  );
}

const ReasoningTraceHeader = ({ isStreaming, steps, toggleCollapse, collapsed }: Props) => {
  const isComplete = !isStreaming && steps.length > 0;

  return (
    <button type='button' onClick={toggleCollapse} className='mb-1 flex w-full items-center gap-2'>
      <TraceHeaderIcon isComplete={isComplete} collapsed={collapsed} />
      <span className='font-medium text-muted-foreground text-sm'>Reasoning trace</span>
      <span className='ml-auto font-mono text-muted-foreground text-xs'>
        {formatProgress(steps, isComplete)}
      </span>
    </button>
  );
};

export default ReasoningTraceHeader;
