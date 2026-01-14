import { Card } from "@/components/ui/shadcn/card";
import { Badge } from "@/components/ui/shadcn/badge";
import { AlertCircle, CheckCircle2, Clock, Coins, Timer } from "lucide-react";
import type { Trace } from "@/services/api/traces";
import {
  getAgentRef,
  getPrompt,
  getDurationMs,
  getTokensTotal,
  getSpanAttributesAsRecord,
} from "@/services/api/traces";
import {
  formatDuration,
  formatTimeAgo,
  formatSpanLabel,
  SpanIcon,
} from "../../utils";

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
    <Card
      className="px-3 py-2 hover:bg-accent cursor-pointer transition-colors"
      onClick={onClick}
    >
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          {isError ? (
            <AlertCircle className="h-4 w-4 text-destructive flex-shrink-0" />
          ) : (
            <CheckCircle2 className="h-4 w-4 text-green-500 flex-shrink-0" />
          )}
          <SpanIcon
            spanName={trace.spanName}
            className="h-4 w-4 flex-shrink-0 text-muted-foreground"
          />
          <span className="flex-1 truncate text-sm font-medium">
            {displayTitle}
          </span>
          <span className="flex items-center gap-1 text-xs text-muted-foreground flex-shrink-0">
            <Clock className="h-3 w-3" />
            {formatTimeAgo(trace.timestamp)}
          </span>
        </div>
        <div className="flex items-center gap-2 ml-6">
          {/* Show span type label */}
          <Badge variant="outline" className="text-xs">
            {formatSpanLabel(trace.spanName)}
          </Badge>

          {/* Show agent ref for agent traces */}
          {!isWorkflow && agentRef && (
            <span className="text-xs text-muted-foreground">{agentRef}</span>
          )}

          {/* Show workflow ref for workflow traces */}
          {isWorkflow && workflowRef && (
            <span className="text-xs text-muted-foreground">{workflowRef}</span>
          )}

          <Badge variant="secondary" className="text-xs gap-1">
            <Timer className="h-3 w-3" />
            {formatDuration(durationMs)}
          </Badge>

          {!!tokensTotal && tokensTotal !== 0 && (
            <Badge variant="outline" className="text-xs gap-1">
              <Coins className="h-3 w-3" />
              {tokensTotal.toLocaleString()}
            </Badge>
          )}
        </div>
      </div>
    </Card>
  );
}
