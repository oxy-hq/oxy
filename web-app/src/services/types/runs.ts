import { Block, GroupArtifactType, GroupWorkflowType } from "./blocks";

export type WorkflowRetryParam = {
  run_id: number;
  // Formatted as "{parent_task_id}[.{child_task_id}][-{loop_index}]" or "" empty for all tasks
  replay_id: string;
};

export type CreateRunPayload = {
  type: "workflow";
  workflowId: string;
  variables?: Record<string, unknown>;
  retry_param?: WorkflowRetryParam;
};

export type CreateRunResponse = {
  run: RunInfo;
};

export type StreamEventsPayload = {
  sourceId: string;
  runIndex: number;
};

export type RunStatus =
  | "pending"
  | "running"
  | "cancelled"
  | "completed"
  | "failed";

export type RunInfo = {
  source_id: string;
  run_index: number;
  status: RunStatus;
  created_at: string;
  updated_at: string;
};

export type Pagination = {
  size: number;
  page: number;
  num_pages: number;
};

export type ListRunsResponse = {
  items: RunInfo[];
  pagination: Pagination;
};

export type GetBlocksRequest = {
  source_id: string;
  run_index?: number;
};

export type GroupKind = GroupArtifactType | GroupWorkflowType;

export type GetBlocksResponse = RunInfo & {
  metadata?: GroupKind;
  blocks?: Record<string, Block>;
  children?: string[];
  error?: string;
};

export type TaskRun = {
  id: string;
  name: string;
  children: string[];
  isStreaming?: boolean;
  error?: string;
  // Optional, only present if the task is a loop item
  loopIndex?: number;
  // Optional, only present if the task is a loop
  loopValue?: unknown[];
  // Optional, only present if the task is a subworkflow
  subWorkflowRunId?: number;
};
