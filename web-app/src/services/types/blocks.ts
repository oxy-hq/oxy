import { WorkflowConfig } from "@/stores/useWorkflow";

export type WorkflowContent = {
  type: "workflow";
  workflow_id: string;
  run_id: string;
  workflow_config: WorkflowConfig;
};

export type TaskSubWorkflowMetadata = {
  type: "sub_workflow";
  workflow_id: string;
  run_id: number;
};

export type TaskLoopMetadata = {
  type: "loop";
  values: unknown[];
};

export type TaskLoopItemMetadata = {
  type: "loop_item";
  index: number;
};

export type TaskMetadata =
  | TaskSubWorkflowMetadata
  | TaskLoopMetadata
  | TaskLoopItemMetadata;

export type TaskContent = {
  type: "task";
  task_name: string;
  task_metadata?: TaskMetadata;
};

export type TextContent = {
  type: "text";
  content: string;
};

export type SqlContent = {
  type: "sql";
  database: string;
  sql_query: string;
  result: string[][];
  is_result_truncated: boolean;
};

export type GroupContent = {
  type: "group";
  group_id: string;
};

export type ArtifactWorkflowMetadata = {
  type: "workflow";
  workflow_id: string;
};

export type ArtifactAgentMetadata = {
  type: "agent";
  agent_id: string;
};

export type ArtifactSqlMetadata = {
  type: "execute_sql";
  database: string;
};

export type ArtifactMetadata =
  | ArtifactAgentMetadata
  | ArtifactWorkflowMetadata
  | ArtifactSqlMetadata;

export type ArtifactContent = {
  type: "artifact";
  artifact_name: string;
  is_verified: boolean;
  artifact_metadata: ArtifactMetadata;
};

export type BlockContent =
  | TaskContent
  | TextContent
  | SqlContent
  | GroupContent;

export type Block = {
  id: string;
  children: string[];
  error?: string;
  is_streaming?: boolean;
} & BlockContent;

export type GroupWorkflowType = {
  type: "workflow";
  workflow_id: string;
  run_id: string;
  workflow_config?: WorkflowConfig;
};

export type GroupArtifactType = {
  type: "artifact";
  artifact_id: string;
  artifact_name: string;
  artifact_metadata: ArtifactMetadata;
  is_verified: boolean;
};

export type Group = {
  id: string;
  error?: string;
  is_streaming?: boolean;
} & (GroupWorkflowType | GroupArtifactType);

export type WorkflowStartedEvent = {
  type: "workflow_started";
  workflow_id: string;
  run_id: string;
  workflow_config: WorkflowConfig;
  variables?: Record<string, unknown>;
};

export type WorkflowFinishedEvent = {
  type: "workflow_finished";
  workflow_id: string;
  run_id: string;
  error?: string;
};

export type WorkflowEvent = WorkflowStartedEvent | WorkflowFinishedEvent;

export type TaskStartedEvent = {
  type: "task_started";
  task_id: string;
  task_name: string;
  task_metadata?: TaskMetadata;
};
export type TaskFinishedEvent = {
  type: "task_finished";
  task_id: string;
  error?: string;
};
export type TaskMetadataEvent = {
  type: "task_metadata";
  task_id: string;
  metadata: TaskMetadata;
};

export type TaskEvent =
  | TaskStartedEvent
  | TaskFinishedEvent
  | TaskMetadataEvent;

export type ContentEvent = {
  type: "content_added" | "content_done";
  content_id: string;
  item: TextContent | SqlContent;
};

export type ArtifactStartedEvent = {
  type: "artifact_started";
  artifact_id: string;
  artifact_name: string;
  is_verified: boolean;
  artifact_metadata: ArtifactMetadata; // e.g., "execute_sql", "agent", "workflow"
};

export type ArtifactFinishedEvent = {
  type: "artifact_finished";
  artifact_id: string;
  error?: string;
};

export type ArtifactEvent = ArtifactStartedEvent | ArtifactFinishedEvent;

export type BlockEvent =
  | WorkflowEvent
  | TaskEvent
  | ContentEvent
  | ArtifactEvent;
