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

export enum MetricKind {
  Accuracy = "Accuracy",
}

export type MetricValue = {
  [MetricKind.Accuracy]: number;
};

export type Metric = {
  errors: string[];
  records: Record[];
  kind: MetricValue;
};
