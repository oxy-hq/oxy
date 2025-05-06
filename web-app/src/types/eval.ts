export const EVAL_METRICS_POSTFIX = "metrics";

export enum EvalEventState {
  Started = "Started",
  Progress = "Progress",
  Finished = "Finished",
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
}

export interface RecallRecord {
  score: number;
  pass: boolean;
  retrieved_contexts: string[];
  reference_contexts: string[];
}

export enum MetricKind {
  Similarity = "Similarity",
  Recall = "Recall",
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

export type MetricValue = SimilarityMetric | RecallMetric;

export type Metric = {
  errors: string[];
  metrics: MetricValue[];
};
