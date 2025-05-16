import { create } from "zustand";

import { LogItem } from "@/hooks/api/runWorkflow";

export type NodeData = {
  task: TaskConfigWithId;
  id: string;
  index: number;
  metadata?: Record<string, unknown>;
};

export type TaskConfigWithId = (
  | ExecuteSqlTaskConfig
  | FormatterTaskConfig
  | AgentTaskConfig
  | LoopSequentialTaskConfigWithId
  | WorkflowTaskConfig
  | ConditionalTaskConfigWithId
) & { id: string };

export type WorkflowConfigWithPath = Omit<WorkflowConfig, "tasks"> & {
  path: string;
  tasks: TaskConfigWithId[];
};

export type LayoutedNode = {
  id: string;
  size: {
    width: number;
    height: number;
  };
  position: {
    x: number;
    y: number;
  };
  data: NodeData;
  parentId?: string;
  hidden?: boolean;
};

export type Node = {
  id: string;
  parentId?: string;
  name: string;
  type: NodeType;
  hidden: boolean;
  data: NodeData;
  width: number;
  height: number;
  children: Node[];
};

export type Edge = {
  id: string;
  source: string;
  target: string;
  hidden?: boolean;
};

export type WorkflowConfig = {
  id: string;
  name: string;
  tasks: TaskConfig[];
  path?: string;
};

interface WorkflowState {
  nodes: Node[];
  edges: Edge[];
  selectedNodeId: string | null;
  layoutedNodes: LayoutedNode[];
  setNodes: (nodes: Node[]) => void;
  setEdges: (edges: Edge[]) => void;
  setLayoutedNodes: (layoutedNodes: LayoutedNode[]) => void;
  setNodeVisibility: (id: string[], visible: boolean) => void;
  setSelectedNodeId(nodeId: string | null): void;
  workflow: WorkflowConfigWithPath | null;
  setWorkflow: (workflow: WorkflowConfigWithPath) => void;
  logs: LogItem[];
  setLogs: (logs: LogItem[]) => void;
  appendLog: (log: LogItem) => void;
  appendLogs: (logs: LogItem[]) => void;
}

const useWorkflow = create<WorkflowState>((set, get) => ({
  setLogs: (logs) => set({ logs }),
  appendLog: (log) => set((state) => ({ logs: [...state.logs, log] })),
  logs: [],
  nodes: [],
  workflow: null,
  setWorkflow: (workflow) => set({ workflow }),
  edges: [],
  selectedNodeId: null,
  layoutedNodes: [],
  setNodes: (nodes: Node[]) => set({ nodes }),
  setEdges: (edges: Edge[]) => set({ edges }),
  setLayoutedNodes(layoutedNodes) {
    set({ layoutedNodes });
  },
  setNodeVisibility: (ids: string[], visible: boolean) => {
    set((state) => {
      // Create a Set for faster lookup of node IDs
      const nodeIds = new Set(ids);
      const newNodes = state.nodes.map((node) => {
        // Check if the node or its parent is in the Set
        if (nodeIds.has(node.id) || nodeIds.has(node.parentId!)) {
          // Add the node's ID to the Set to handle its children
          nodeIds.add(node.id);
          // Return a new node object with updated hidden property
          return { ...node, hidden: !visible };
        }
        // Return the node unchanged if it doesn't match the criteria
        return node;
      });

      // Return the updated state with the new nodes
      return { ...state, nodes: newNodes };
    });
  },
  setSelectedNodeId(nodeId: string | null) {
    set({ selectedNodeId: nodeId });
  },
  getSelectedNode() {
    return get().nodes.find((node) => node.id === get().selectedNodeId) || null;
  },
  appendLogs: (logs) => {
    set((state) => ({ logs: [...state.logs, ...logs] }));
  },
}));

export enum TaskType {
  EXECUTE_SQL = "execute_sql",
  FORMATTER = "formatter",
  AGENT = "agent",
  LOOP_SEQUENTIAL = "loop_sequential",
  WORKFLOW = "workflow",
  CONDITIONAL = "conditional",
}

export enum NoneTaskNodeType {
  CONDITIONAL_ELSE = "conditional-else",
  CONDITIONAL_IF = "conditional-if",
}

export type NodeType = TaskType | NoneTaskNodeType;

export type BaseTaskConfig = {
  name: string;
  type: TaskType;
  export?: ExportConfig;
};

export type ExportFormat = "csv" | "json" | "sql" | "docx";

export type ExportConfig = {
  format: ExportFormat;
  path: string;
};

// Specific task configurations
export type FormatterTaskConfig = BaseTaskConfig & {
  type: TaskType.FORMATTER;
  template: string;
};

export type AgentTaskConfig = BaseTaskConfig & {
  type: TaskType.AGENT;
  prompt: string;
  agent_ref: string;
};

export type WorkflowTaskConfig = BaseTaskConfig & {
  type: TaskType.WORKFLOW;
  src: string;
};

export type LoopSequentialTaskConfig = BaseTaskConfig & {
  type: TaskType.LOOP_SEQUENTIAL;
  tasks: TaskConfig[];
  values: string | string[];
};

export type ConditionConfigWithId = {
  if: string;
  tasks: TaskConfigWithId[];
};

export type ConditionConfig = {
  if: string;
  tasks: TaskConfig[];
};

export type ConditionalTaskConfigWithId = BaseTaskConfig & {
  type: TaskType.CONDITIONAL;
  conditions: ConditionConfigWithId[];
  else?: TaskConfigWithId[];
};

export type ConditionalTaskConfig = BaseTaskConfig & {
  type: TaskType.CONDITIONAL;
  conditions: ConditionConfig[];
  else?: TaskConfig[];
};

export type LoopSequentialTaskConfigWithId = BaseTaskConfig & {
  type: TaskType.LOOP_SEQUENTIAL;
  tasks: TaskConfigWithId[];
  values: string | string[];
};

export type ExecuteSqlTaskConfig = BaseTaskConfig & {
  type: TaskType.EXECUTE_SQL;
  sql?: string;
  sql_file?: string;
  database: string;
};

// Unified TaskConfig type with discriminated union
export type TaskConfig =
  | ExecuteSqlTaskConfig
  | FormatterTaskConfig
  | AgentTaskConfig
  | LoopSequentialTaskConfig
  | ConditionalTaskConfig
  | WorkflowTaskConfig;

export default useWorkflow;
