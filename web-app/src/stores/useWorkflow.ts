import {
  applyEdgeChanges,
  applyNodeChanges,
  type Edge,
  type Node,
  type OnEdgesChange,
  type OnNodesChange
} from "@xyflow/react";
import { create } from "zustand";
import { buildWorkflowNodes } from "@/components/workflow/WorkflowDiagram/layout/nodeBuilder";

export type NodeData = {
  task: TaskConfigWithId;
  id: string;
  index: number;
  metadata?: Record<string, unknown>;
  expanded?: boolean;
};

export enum TaskType {
  EXECUTE_SQL = "execute_sql",
  SEMANTIC_QUERY = "semantic_query",
  LOOKER_QUERY = "looker_query",
  OMNI_QUERY = "omni_query",
  FORMATTER = "formatter",
  LOOP_SEQUENTIAL = "loop_sequential",
  WORKFLOW = "workflow",
  CONDITIONAL = "conditional",
  VISUALIZE = "visualize"
}

export enum NoneTaskNodeType {
  CONDITIONAL_ELSE = "conditional-else",
  CONDITIONAL_IF = "conditional-if"
}

export type NodeType = TaskType | NoneTaskNodeType;

type BaseTaskConfig = {
  name: string;
  type: TaskType;
  export?: ExportConfig;
};

type ExportFormat = "csv" | "json" | "sql" | "docx";

type ExportConfig = {
  format: ExportFormat;
  path: string;
};

// Specific task configurations
type FormatterTaskConfig = BaseTaskConfig & {
  type: TaskType.FORMATTER;
  template: string;
};

export type WorkflowTaskConfig = BaseTaskConfig & {
  type: TaskType.WORKFLOW;
  src: string;
  tasks?: TaskConfig[];
};

export type LoopSequentialTaskConfig = BaseTaskConfig & {
  type: TaskType.LOOP_SEQUENTIAL;
  tasks: TaskConfig[];
  values: string | string[];
};

type ConditionConfigWithId = {
  if: string;
  tasks: TaskConfigWithId[];
};

type ConditionConfig = {
  if: string;
  tasks: TaskConfig[];
};

export type ConditionalTaskConfigWithId = BaseTaskConfig & {
  type: TaskType.CONDITIONAL;
  conditions: ConditionConfigWithId[];
  else?: TaskConfigWithId[];
};

type ConditionalTaskConfig = BaseTaskConfig & {
  type: TaskType.CONDITIONAL;
  conditions: ConditionConfig[];
  else?: TaskConfig[];
};

type LoopSequentialTaskConfigWithId = BaseTaskConfig & {
  type: TaskType.LOOP_SEQUENTIAL;
  tasks: TaskConfigWithId[];
  values: string | string[];
};

export type WorkflowTaskConfigWithId = BaseTaskConfig & {
  type: TaskType.WORKFLOW;
  src: string;
  tasks?: TaskConfigWithId[];
};

type ExecuteSqlTaskConfig = BaseTaskConfig & {
  type: TaskType.EXECUTE_SQL;
  sql_query?: string;
  sql_file?: string;
  database: string;
};

type SemanticQueryTaskConfig = BaseTaskConfig & {
  type: TaskType.SEMANTIC_QUERY;
  database: string;
  topic: string;
  dimensions?: string[];
  measures?: string[];
  filters?: Array<{
    field: string;
    op: string;
    value: string | number | boolean | string[];
  }>;
  orders?: Array<{
    field: string;
    direction: string;
  }>;
  limit?: number;
  offset?: number;
};

type OmniQueryTaskConfig = BaseTaskConfig & {
  type: TaskType.OMNI_QUERY;
  integration: string;
  topic: string;
  fields: string[];
  limit?: number;
  sorts?: Record<string, string>;
};

type LookerQueryTaskConfig = BaseTaskConfig & {
  type: TaskType.LOOKER_QUERY;
  integration: string;
  model: string;
  explore: string;
  fields?: string[];
  filters?: Array<{
    key: string;
    value: string;
  }>;
  filter_expression?: string;
  sorts?: Array<{
    field: string;
    direction: string;
  }>;
  limit?: number;
};

type VisualizeTaskConfig = BaseTaskConfig & {
  type: TaskType.VISUALIZE;
  prompt?: string;
};

// Unified TaskConfig type with discriminated union
export type TaskConfig =
  | ExecuteSqlTaskConfig
  | SemanticQueryTaskConfig
  | LookerQueryTaskConfig
  | OmniQueryTaskConfig
  | FormatterTaskConfig
  | LoopSequentialTaskConfig
  | ConditionalTaskConfig
  | WorkflowTaskConfig
  | VisualizeTaskConfig;

export type TaskConfigWithId = (
  | ExecuteSqlTaskConfig
  | SemanticQueryTaskConfig
  | LookerQueryTaskConfig
  | OmniQueryTaskConfig
  | FormatterTaskConfig
  | LoopSequentialTaskConfigWithId
  | WorkflowTaskConfigWithId
  | ConditionalTaskConfigWithId
  | VisualizeTaskConfig
) & {
  id: string;
  workflowId: string;
  runId?: string;
  subWorkflowTaskId?: string;
};

export type WorkflowConfig = {
  id: string;
  name: string;
  tasks: TaskConfig[];
  path?: string;
  variables?: Record<string, unknown>;
};

export type TaskNode = Node<NodeData, NodeType>;

type WorkflowState = {
  baseNodes: TaskNode[];
  nodes: TaskNode[];
  edges: Edge[];
  selectedNodeId?: string;

  setNodes: (nodes: TaskNode[]) => void;
  onNodesChange: OnNodesChange<TaskNode>;
  onEdgesChange: OnEdgesChange;

  initFromTasks: (tasks: TaskConfigWithId[]) => void;
  setNodeExpanded: (nodeId: string, expanded: boolean) => void;
};

const useWorkflow = create<WorkflowState>((set, get) => ({
  baseNodes: [],
  nodes: [],
  edges: [],

  setNodes: (nodes: TaskNode[]) => set({ nodes }),
  onNodesChange: (changes) => {
    set({
      nodes: applyNodeChanges(changes, get().nodes)
    });
  },
  onEdgesChange: (changes) => {
    set({
      edges: applyEdgeChanges(changes, get().edges)
    });
  },
  initFromTasks: async (tasks: TaskConfigWithId[]) => {
    const { nodes, edges } = buildWorkflowNodes(tasks);
    set({
      baseNodes: nodes,
      edges
    });
  },
  setNodeExpanded: async (nodeId: string, expanded: boolean) => {
    const nodes = get().baseNodes.map((node) => {
      if (node.id === nodeId) {
        return {
          ...node,
          data: { ...node.data, expanded }
        };
      }

      return node;
    });

    set({
      baseNodes: nodes
    });
  }
}));

export default useWorkflow;
