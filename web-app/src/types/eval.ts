export enum EvalEventState {
  Started = "Started",
  GeneratingOutputs = "GeneratingOutputs",
  SomeOutputsFailed = "SomeOutputsFailed",
  EvaluatingRecords = "EvaluatingRecords",
  Finished = "Finished",
  Workflow = "Workflow",
  Agent = "Agent",
}

export interface TestStreamMessage {
  error: string | null;
  event: EvalEvent | null;
}

export type EvalEvent =
  | { type: EvalEventState.Started }
  | { type: EvalEventState.GeneratingOutputs; progress: number; total: number }
  | {
      type: EvalEventState.SomeOutputsFailed;
      failed_count: number;
      err: string;
    }
  | { type: EvalEventState.EvaluatingRecords; progress: number; total: number }
  | {
      type: EvalEventState.Finished;
      metrics: Metrics;
      records: Record[];
    }
  | { type: EvalEventState.Workflow; event: unknown }
  | { type: EvalEventState.Agent; event: unknown };

export interface Record {
  cot: string;
  choice: string;
  score: number;
}

export type Metrics = {
  Accuracy: number;
};
