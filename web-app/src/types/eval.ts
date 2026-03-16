export const EVAL_METRICS_POSTFIX = "metrics";

export enum EvalEventState {
  Started = "Started",
  Progress = "Progress",
  Finished = "Finished"
}

export interface TestStreamMessage {
  error: string | null;
  event: EvalEvent | null;
}

export type EvalEvent =
  | { type: EvalEventState.Started }
  | {
      type: EvalEventState.Progress;
      id: string;
      progress: number;
      total: number;
    }
  | {
      type: EvalEventState.Finished;
      metric: Metric;
    };

export interface Record {
  cot: string;
  choice: string;
  score: number;
  prompt?: string;
  expected?: string;
  actual_output?: string;
  references?: unknown[];
  duration_ms: number;
  input_tokens: number;
  output_tokens: number;
}

export interface RecallRecord {
  score: number;
  pass: boolean;
  retrieved_contexts: string[];
  reference_contexts: string[];
}

export interface RunStats {
  /** Total runs attempted, including those that errored out */
  total_attempted: number;
  /** Runs that produced output (didn't crash) */
  answered: number;
}

export enum MetricKind {
  Similarity = "Similarity",
  Recall = "Recall",
  Correctness = "Correctness"
}

export type SimilarityMetric = {
  type: MetricKind.Similarity;
  score: number;
  records: Record[];
};

export type RecallMetric = {
  type: MetricKind.Recall;
  score: number;
  records: RecallRecord[];
};

export type CorrectnessMetric = {
  type: MetricKind.Correctness;
  score: number;
  records: Record[];
};

export type MetricValue = SimilarityMetric | RecallMetric | CorrectnessMetric;

export type Metric = {
  errors: string[];
  metrics: MetricValue[];
  stats: RunStats;
};
