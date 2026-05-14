import type { WorkflowConfig } from "@/stores/useWorkflow";

type TaskSubWorkflowMetadata = {
  type: "sub_workflow";
  workflow_id: string;
  run_id: number;
};

type TaskLoopMetadata = {
  type: "loop";
  values: unknown[];
};

type TaskLoopItemMetadata = {
  type: "loop_item";
  index: number;
};

type TaskMetadata = TaskSubWorkflowMetadata | TaskLoopMetadata | TaskLoopItemMetadata;

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

type SemanticQueryContent = {
  type: "semantic_query";
  semantic_query: string;
  sql_query?: string;
  results?: string[][];
};

type LookerQueryContent = {
  type: "looker_query";
  integration: string;
  model: string;
  explore: string;
  sql_query?: string;
  sql?: string;
  fields: string[];
  filters?: Record<string, string>;
  sorts?: string[];
  limit?: number;
  result?: string[][];
  result_file?: string;
  is_result_truncated: boolean;
};

type SqlContent = {
  type: "sql";
  database: string;
  sql_query: string;
  result: string[][];
  is_result_truncated: boolean;
};

type VizContent = {
  type: "viz";
  name: string;
  title: string;
  config: unknown;
};

type GroupContent = {
  type: "group";
  group_id: string;
};

type DataAppContent = {
  type: "data_app";
  file_path: string;
};

type ArtifactWorkflowMetadata = {
  type: "workflow";
  workflow_id: string;
};

type ArtifactAgentMetadata = {
  type: "agent";
  agent_id: string;
};

type ArtifactSqlMetadata = {
  type: "execute_sql";
  database: string;
};

type ArtifactMetadata = ArtifactAgentMetadata | ArtifactWorkflowMetadata | ArtifactSqlMetadata;

export type BlockContent =
  | TaskContent
  | StepContent
  | TextContent
  | SqlContent
  | SemanticQueryContent
  | LookerQueryContent
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

type WorkflowStartedEvent = {
  type: "workflow_started";
  workflow_id: string;
  run_id: string;
  workflow_config: WorkflowConfig;
  variables?: Record<string, unknown>;
};

type WorkflowFinishedEvent = {
  type: "workflow_finished";
  workflow_id: string;
  run_id: string;
  error?: string;
};

type WorkflowEvent = WorkflowStartedEvent | WorkflowFinishedEvent;

type TaskStartedEvent = {
  type: "task_started";
  task_id: string;
  task_name: string;
  task_metadata?: TaskMetadata;
};
type TaskFinishedEvent = {
  type: "task_finished";
  task_id: string;
  error?: string;
};
type TaskMetadataEvent = {
  type: "task_metadata";
  task_id: string;
  metadata: TaskMetadata;
};

type TaskEvent = TaskStartedEvent | TaskFinishedEvent | TaskMetadataEvent;

type AgenticEvent = AgenticStartedEvent | AgenticFinishedEvent;

type AgenticStartedEvent = {
  type: "agentic_started";
  agent_id: string;
  run_id: string;
};

type AgenticFinishedEvent = {
  type: "agentic_finished";
  agent_id: string;
  run_id: string;
  error?: string;
};

type StepEvent = StepStartedEvent | StepFinishedEvent;

export type StepType =
  | "idle"
  | "plan"
  | "route"
  | "end"
  | "query"
  | "semantic_query"
  | "looker_query"
  | "visualize"
  | "insight"
  | "subflow"
  | "build_app"
  | "save_automation";

type StepStartedEvent = {
  type: "step_started";
  id: string;
  step_type: StepType;
  objective?: string;
};

type StepFinishedEvent = {
  type: "step_finished";
  step_id: string;
  error?: string;
};

type ContentEvent = {
  type: "content_added" | "content_done";
  content_id: string;
  item: TextContent | SqlContent | SemanticQueryContent | LookerQueryContent;
};

type ArtifactStartedEvent = {
  type: "artifact_started";
  artifact_id: string;
  artifact_name: string;
  is_verified: boolean;
  artifact_metadata: ArtifactMetadata; // e.g., "execute_sql", "agent", "workflow"
};

type ArtifactFinishedEvent = {
  type: "artifact_finished";
  artifact_id: string;
  error?: string;
};

type ArtifactEvent = ArtifactStartedEvent | ArtifactFinishedEvent;

export type BlockEvent =
  | WorkflowEvent
  | TaskEvent
  | AgenticEvent
  | StepEvent
  | ContentEvent
  | ArtifactEvent;
