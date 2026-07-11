import type { Trace } from "@/services/api/traces";
import {
  getAgentRef,
  getDurationMs,
  getPrompt,
  getSpanAttributesAsRecord,
  getTokensTotal
} from "@/services/api/traces";
import { formatSpanLabel } from "../../utils";

/** Flattened, display-ready view of a root trace — shared by the card and table renderers. */
export interface TraceRowView {
  traceId: string;
  isError: boolean;
  isAutomation: boolean;
  isAnalytics: boolean;
  /** Human span-type label, e.g. "Agent" / "Automation". */
  spanLabel: string;
  /** Prompt / question / automation ref, falling back to the span label. */
  title: string;
  /** Agent ref (agent/analytics traces) or automation ref (automation traces). */
  entityRef?: string;
  durationMs: number;
  tokensTotal?: number;
  timestamp: string;
}

export function deriveTraceRow(trace: Trace): TraceRowView {
  const attrs = getSpanAttributesAsRecord(trace);
  const isAutomation = trace.spanName.startsWith("workflow.");
  const isAnalytics = trace.spanName === "analytics.run";
  const automationRef = attrs["oxy.workflow.ref"];
  const agentRef = getAgentRef(trace);
  const prompt = isAnalytics ? attrs.question : getPrompt(trace);
  const spanLabel = formatSpanLabel(trace.spanName);
  return {
    traceId: trace.traceId,
    isError: trace.statusCode === "Error",
    isAutomation,
    isAnalytics,
    spanLabel,
    title: isAutomation ? automationRef || spanLabel : prompt || spanLabel,
    entityRef: isAutomation ? automationRef : agentRef,
    durationMs: getDurationMs(trace),
    tokensTotal: getTokensTotal(trace),
    timestamp: trace.timestamp
  };
}
