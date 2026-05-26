import type { Trace } from "@/services/api/traces";

export interface TraceChartsProps {
  traces: Trace[] | undefined;
  isLoading: boolean;
}

export interface TimeBucket {
  time: string;
  workflowCount: number;
  analyticsCount: number;
  tokens: number;
}

export interface DurationBucket {
  range: string;
  count: number;
}

export interface TraceStats {
  workflowRuns: number;
  analyticsRuns: number;
  avgDuration: string;
  totalTokens: number;
}
