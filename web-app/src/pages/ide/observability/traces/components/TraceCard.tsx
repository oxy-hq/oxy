import { AlertCircle, CheckCircle2, Clock, Coins, Timer } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Card } from "@/components/ui/shadcn/card";
import type { Trace } from "@/services/api/traces";
import {
  getAgentRef,
  getDurationMs,
  getPrompt,
  getSpanAttributesAsRecord,
  getTokensTotal
} from "@/services/api/traces";
import { formatDuration, formatSpanLabel, formatTimeAgo, SpanIcon } from "../../utils";

interface TraceCardProps {
  trace: Trace;
  onClick: () => void;
}

// Helper to get workflow reference from trace attributes
function getWorkflowRef(trace: Trace): string | undefined {
  const attrs = getSpanAttributesAsRecord(trace);
  return attrs["oxy.workflow.ref"];
}

// Helper to determine if trace is a workflow
function isWorkflowTrace(trace: Trace): boolean {
  return trace.spanName.startsWith("workflow.");
}

export function TraceCard({ trace, onClick }: TraceCardProps) {
  const isError = trace.statusCode === "Error";
  const isWorkflow = isWorkflowTrace(trace);
  const agentRef = getAgentRef(trace);
  const workflowRef = getWorkflowRef(trace);
  const prompt = getPrompt(trace);
  const durationMs = getDurationMs(trace);
  const tokensTotal = getTokensTotal(trace);

  // For workflow traces, use workflow_ref; for agent traces, use prompt
  const displayTitle = isWorkflow
    ? workflowRef || formatSpanLabel(trace.spanName)
    : prompt || formatSpanLabel(trace.spanName);

  return (
    <Card className='cursor-pointer px-3 py-2 transition-colors hover:bg-accent' onClick={onClick}>
      <div className='flex flex-col gap-1'>
        <div className='flex items-center gap-2'>
          {isError ? (
            <AlertCircle className='h-4 w-4 flex-shrink-0 text-destructive' />
          ) : (
            <CheckCircle2 className='h-4 w-4 flex-shrink-0 text-green-500' />
          )}
          <SpanIcon
            spanName={trace.spanName}
            className='h-4 w-4 flex-shrink-0 text-muted-foreground'
          />
          <span className='flex-1 truncate font-medium text-sm'>{displayTitle}</span>
          <span className='flex flex-shrink-0 items-center gap-1 text-muted-foreground text-xs'>
            <Clock className='h-3 w-3' />
            {formatTimeAgo(trace.timestamp)}
          </span>
        </div>
        <div className='ml-6 flex items-center gap-2'>
          {/* Show span type label */}
          <Badge variant='outline' className='text-xs'>
            {formatSpanLabel(trace.spanName)}
          </Badge>

          {/* Show agent ref for agent traces */}
          {!isWorkflow && agentRef && (
            <span className='text-muted-foreground text-xs'>{agentRef}</span>
          )}

          {/* Show workflow ref for workflow traces */}
          {isWorkflow && workflowRef && (
            <span className='text-muted-foreground text-xs'>{workflowRef}</span>
          )}

          <Badge variant='secondary' className='gap-1 text-xs'>
            <Timer className='h-3 w-3' />
            {formatDuration(durationMs)}
          </Badge>

          {!!tokensTotal && tokensTotal !== 0 && (
            <Badge variant='outline' className='gap-1 text-xs'>
              <Coins className='h-3 w-3' />
              {tokensTotal.toLocaleString()}
            </Badge>
          )}
        </div>
      </div>
    </Card>
  );
}
