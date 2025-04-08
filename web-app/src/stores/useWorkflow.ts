import { NodeType } from "./useWorkflow";
import { create } from "zustand";
import debounce from "debounce";

import { LogItem } from "@/hooks/api/runWorkflow";

export type NodeData = {
  task: TaskConfigWithId;
  id: string;
  index: number;
  canMoveUp: boolean;
  canMoveDown: boolean;
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
  size: {
    width: number;
    height: number;
  };
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
  updateNode: (node: Node) => void;
  upsertNode: (node: Node) => void;
  setEdges: (edges: Edge[]) => void;
  setLayoutedNodes: (layoutedNodes: LayoutedNode[]) => void;
  getNode: (id: string) => Node | null;
  setNodeVisibility: (id: string[], visible: boolean) => void;
  setSelectedNodeId(nodeId: string | null): void;
  getSelectedNode: () => Node | null;
  workflow: WorkflowConfigWithPath | null;
  setWorkflow: (workflow: WorkflowConfigWithPath) => void;
  updateTask: (
    id: string,
    data: Partial<Omit<TaskConfigWithId, "id" | "type">>,
  ) => void;
  saveWorkflow: () => void;
  moveTaskUp: (id: string) => void;
  moveTaskDown: (id: string) => void;
  getAllParentIds: (id: string) => Set<string>;
  removeTask: (id: string) => void;
  running: boolean;
  setRunning: (running: boolean) => void;
  logs: LogItem[];
  runWorkflow: () => void;
  setLogs: (logs: LogItem[]) => void;
  appendLog: (log: LogItem) => void;
  appendLogs: (logs: LogItem[]) => void;
}

const findAndUpdateTask = (
  tasks: TaskConfigWithId[],
  id: string,
  data: Partial<Omit<TaskConfigWithId, "id" | "type">>,
): TaskConfigWithId[] => {
  return tasks.map((task) => {
    if (task.id === id) {
      return { ...task, ...data };
    }
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return { ...task, tasks: findAndUpdateTask(task.tasks, id, data) };
    }
    return task;
  });
};

const removeTaskIds = (tasks: TaskConfigWithId[]): TaskConfig[] => {
  return tasks.map((task) => {
    if (task.type === TaskType.LOOP_SEQUENTIAL) {
      return { ...task, steps: removeTaskIds(task.tasks), id: undefined };
    }
    if (task.type === TaskType.CONDITIONAL) {
      return {
        ...task,
        conditions: task.conditions.map((condition) => ({
          ...condition,
          tasks: removeTaskIds(condition.tasks),
        })),
        else: removeTaskIds(task.else),
        id: undefined,
      };
    }
    return { ...task, id: undefined };
  });
};

const findAndMoveTaskDown = (
  tasks: TaskConfigWithId[],
  id: string,
): TaskConfigWithId[] => {
  const stepIndex = tasks.findIndex((step) => step.id === id);
  if (stepIndex === -1)
    return tasks.map((task) => {
      if (task.type === TaskType.LOOP_SEQUENTIAL) {
        return { ...task, tasks: findAndMoveTaskDown(task.tasks, id) };
      }
      return task;
    });
  if (stepIndex === tasks.length - 1) return tasks;
  const newSteps = [...tasks];
  newSteps[stepIndex] = tasks[stepIndex + 1];
  newSteps[stepIndex + 1] = tasks[stepIndex];
  return newSteps;
};
const findAndMoveTaskUp = (
  tasks: TaskConfigWithId[],
  id: string,
): TaskConfigWithId[] => {
  const stepIndex = tasks.findIndex((step) => step.id === id);
  if (stepIndex === -1)
    return tasks.map((task) => {
      if (task.type === TaskType.LOOP_SEQUENTIAL) {
        return { ...task, steps: findAndMoveTaskUp(task.tasks, id) };
      }
      return task;
    });
  if (stepIndex === 0) return tasks;
  const newSteps = [...tasks];
  newSteps[stepIndex] = tasks[stepIndex - 1];
  newSteps[stepIndex - 1] = tasks[stepIndex];
  return newSteps;
};

const useWorkflow = create<WorkflowState>((set, get) => ({
  setLogs: (logs) => set({ logs }),
  appendLog: (log) => set((state) => ({ logs: [...state.logs, log] })),
  running: false,
  setRunning: (running) => set({ running }),
  logs: [],
  runWorkflow: async () => {
    set({ running: true });

    set({ running: false });
  },
  removeTask: (id) => {
    const findAndRemove = (
      tasks: TaskConfigWithId[],
      id: string,
    ): TaskConfigWithId[] => {
      return tasks.reduce((newTasks, task) => {
        if (task.id === id) return [...newTasks];
        if (task.type === TaskType.LOOP_SEQUENTIAL && task.tasks) {
          return [
            ...newTasks,
            { ...task, tasks: findAndRemove(task.tasks, id) },
          ];
        } else {
          return [...newTasks, task];
        }
      }, [] as TaskConfigWithId[]);
    };

    set((state) => {
      const workflow = state.workflow;
      if (!workflow) return state;
      const tasks = findAndRemove([...workflow.tasks], id);
      return { ...state, workflow: { ...workflow, tasks: tasks } };
    });
  },
  getAllParentIds: (id) => {
    const parentIds = new Set<string>();
    let node = get().nodes.find((node) => node.id === id);
    while (node?.parentId) {
      parentIds.add(node.parentId);
      node = get().nodes.find((node) => node.id === node.parentId);
    }
    return parentIds;
  },
  moveTaskDown: (id) => {
    set((state) => {
      const workflow = state.workflow;
      if (!workflow) return state;
      const tasks = findAndMoveTaskDown(workflow.tasks, id);
      return { ...state, workflow: { ...workflow, tasks } };
    });
  },
  moveTaskUp: (id) => {
    set((state) => {
      const workflow = state.workflow;
      if (!workflow) return state;
      const steps = findAndMoveTaskUp(workflow.tasks, id);
      console.log("new steps", JSON.stringify(steps, null, 2));
      return { ...state, workflow: { ...workflow, tasks: steps } };
    });
  },
  saveWorkflow: debounce(async () => {
    const workflow = get().workflow;
    if (!workflow) return;
    const { path, ...workflowWithoutPath } = workflow;
    const dataToSave = {
      ...workflowWithoutPath,
      tasks: removeTaskIds(workflow.tasks),
    };
    //TODO: save the config
    console.log("dataToSave", path, dataToSave);
  }, 500),
  updateTask: (id, data) => {
    set((state) => {
      const workflow = state.workflow;
      if (!workflow) return state;
      const steps = findAndUpdateTask(workflow.tasks, id, data);
      return { ...state, workflow: { ...workflow, tasks: steps } };
    });
  },
  nodes: [],
  workflow: null,
  setWorkflow: (workflow) => set({ workflow }),
  edges: [],
  selectedNodeId: null,
  layoutedNodes: [],
  setNodes: (nodes: Node[]) => set({ nodes }),
  updateNode: (node) =>
    set((state) => {
      const index = state.nodes.findIndex((n) => n.id === node.id);
      const nodes = [...state.nodes];
      nodes[index] = node;
      return { ...state, nodes };
    }),
  upsertNode: (node) => {
    set((state) => {
      const index = state.nodes.findIndex((n) => n.id === node.id);
      if (index === -1) {
        return { nodes: [...state.nodes, node] };
      }
      const nodes = [...state.nodes];
      nodes[index] = node;
      return { ...state, nodes };
    });
  },
  setEdges: (edges: Edge[]) => set({ edges }),
  setLayoutedNodes(layoutedNodes) {
    set({ layoutedNodes });
  },
  getNode: (id: string): Node | null => {
    const findNode = (nodes: Node[], id: string): Node | null => {
      if (!nodes) return null;
      const node = nodes.find((n) => n.id === id);
      if (node) return node;
      for (const n of nodes) {
        const found = findNode(n.children, id);
        if (found) return found;
      }
      return null;
    };
    return findNode(get().nodes, id);
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
  else: TaskConfig[];
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
