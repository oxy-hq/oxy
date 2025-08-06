export enum TaskType {
  EXECUTE_SQL = "execute_sql",
  FORMATTER = "formatter",
  AGENT = "agent",
  LOOP_SEQUENTIAL = "loop_sequential",
  WORKFLOW = "workflow",
}

export type ExportFormat = "csv" | "json" | "sql" | "docx";

export type ExportConfig = {
  format: ExportFormat;
  path: string;
};

export type BaseTaskConfig = {
  name: string;
  type: TaskType;
  export?: ExportConfig;
};

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

export type ExecuteSqlTaskConfig = BaseTaskConfig & {
  type: TaskType.EXECUTE_SQL;
  sql?: string;
  sql_file?: string;
  database: string;
};

export type TaskConfig =
  | ExecuteSqlTaskConfig
  | FormatterTaskConfig
  | AgentTaskConfig
  | LoopSequentialTaskConfig
  | WorkflowTaskConfig;

export type WorkflowConfig = {
  id: string;
  name: string;
  tasks: TaskConfig[];
  path?: string;
};
