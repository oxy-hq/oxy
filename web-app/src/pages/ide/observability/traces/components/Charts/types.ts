import type { Trace } from "@/services/api/traces";

export interface TraceChartsProps {
  traces: Trace[] | undefined;
  isLoading: boolean;
}

export interface TimeBucket {
  time: string;
  agentCount: number;
  workflowCount: number;
  analyticsCount: number;
  tokens: number;
}

export interface DurationBucket {
  range: string;
  count: number;
}

export interface TraceStats {
  agentRuns: number;
  workflowRuns: number;
  analyticsRuns: number;
  avgDuration: string;
  totalTokens: number;
}
