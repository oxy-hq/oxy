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

export type StepContent = {
  type: "step";
  id: string;
  step_type: StepType;
  objective?: string;
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

export type VizContent = {
  type: "viz";
  name: string;
  title: string;
  config: unknown;
};

export type GroupContent = {
  type: "group";
  group_id: string;
};

export type DataAppContent = {
  type: "data_app";
  file_path: string;
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
  | StepContent
  | TextContent
  | SqlContent
  | VizContent
  | DataAppContent
  | GroupContent;

export type BlockBase = {
  id: string;
  children: string[];
  error?: string;
  is_streaming?: boolean;
};

export type Block = BlockBase & BlockContent;

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

export type GroupAgenticType = {
  type: "agentic";
  agent_id: string;
  run_id: string;
};

export type Group = {
  id: string;
  error?: string;
  is_streaming?: boolean;
} & (GroupWorkflowType | GroupArtifactType | GroupAgenticType);

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

export type AgenticEvent = AgenticStartedEvent | AgenticFinishedEvent;

export type AgenticStartedEvent = {
  type: "agentic_started";
  agent_id: string;
  run_id: string;
};

export type AgenticFinishedEvent = {
  type: "agentic_finished";
  agent_id: string;
  run_id: string;
  error?: string;
};

export type StepEvent = StepStartedEvent | StepFinishedEvent;

export type StepType =
  | "idle"
  | "plan"
  | "end"
  | "query"
  | "visualize"
  | "insight"
  | "subflow"
  | "build_app";

export type StepStartedEvent = {
  type: "step_started";
  id: string;
  step_type: StepType;
  objective?: string;
};

export type StepFinishedEvent = {
  type: "step_finished";
  step_id: string;
  error?: string;
};

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
  | AgenticEvent
  | StepEvent
  | ContentEvent
  | ArtifactEvent;
