import {
  AppWindow,
  BookOpen,
  Bot,
  Box,
  Eye,
  FileCode,
  Table,
  Workflow as WorkflowIcon
} from "lucide-react";

export const BORDER_COLORS: Record<string, string> = {
  agent: "var(--graph-agent-border)",
  procedure: "var(--graph-procedure-border)",
  workflow: "var(--graph-procedure-border)",
  app: "var(--graph-app-border)",
  automation: "var(--graph-automation-border)",
  topic: "var(--graph-automation-border)",
  view: "var(--graph-view-border)",
  sql_query: "var(--graph-sql-query-border)",
  table: "var(--graph-table-border)",
  entity: "var(--graph-entity-border)"
};

export const BG_COLORS: Record<string, string> = {
  agent: "var(--graph-agent-bg)",
  procedure: "var(--graph-procedure-bg)",
  workflow: "var(--graph-procedure-bg)",
  app: "var(--graph-app-bg)",
  automation: "var(--graph-automation-bg)",
  topic: "var(--graph-automation-bg)",
  view: "var(--graph-view-bg)",
  sql_query: "var(--graph-sql-query-bg)",
  table: "var(--graph-table-bg)",
  entity: "var(--graph-entity-bg)"
};

export const HANDLE_STYLE_HIDDEN = {
  width: 0,
  height: 0,
  minWidth: 0,
  minHeight: 0,
  opacity: 0,
  border: "none",
  background: "transparent",
  padding: 0
} as const;

export const HANDLE_STYLE_VISIBLE = {
  width: 8,
  height: 8,
  border: "2px solid var(--muted-foreground)",
  background: "var(--background)",
  opacity: 0.6
} as const;

export const ICONS: Record<string, React.ReactNode> = {
  agent: <Bot className='h-3.5 w-3.5' />,
  procedure: <WorkflowIcon className='h-3.5 w-3.5' />,
  workflow: <WorkflowIcon className='h-3.5 w-3.5' />,
  app: <AppWindow className='h-3.5 w-3.5' />,
  automation: <WorkflowIcon className='h-3.5 w-3.5' />,
  topic: <BookOpen className='h-3.5 w-3.5' />,
  view: <Eye className='h-3.5 w-3.5' />,
  sql_query: <FileCode className='h-3.5 w-3.5' />,
  table: <Table className='h-3.5 w-3.5' />,
  entity: <Box className='h-3.5 w-3.5' />
};

export const TYPE_ORDER = [
  "entity",
  "agent",
  "procedure",
  "workflow",
  "app",
  "automation",
  "topic",
  "view",
  "sql_query",
  "table"
];

export const TYPE_LABEL_SINGULAR: Record<string, string> = {
  agent: "Agent",
  workflow: "Workflow",
  procedure: "Procedure",
  topic: "Topic",
  view: "View",
  sql_query: "SQL Query",
  table: "Table",
  entity: "Entity",
  app: "App",
  automation: "Automation"
};

export const TYPE_LABELS: Record<string, string> = {
  agent: "Agents",
  procedure: "Procedures",
  workflow: "Workflows (legacy)",
  automation: "Automations (legacy)",
  topic: "Topics",
  view: "Views",
  sql_query: "SQL Queries",
  table: "Tables",
  entity: "Entities",
  app: "Apps"
};

export type FocusType =
  | "auto"
  | "agent"
  | "procedure"
  | "workflow"
  | "app"
  | "automation"
  | "topic"
  | "view"
  | "sql_query"
  | "table"
  | "entity";

export const FOCUS_OPTIONS: Array<{ value: FocusType; label: string; icon?: React.ReactNode }> = [
  { value: "auto", label: "All Types" },
  { value: "agent", label: "Agents", icon: <Bot className='h-4 w-4' /> },
  { value: "procedure", label: "Procedures", icon: <WorkflowIcon className='h-4 w-4' /> },
  { value: "workflow", label: "Workflows (legacy)", icon: <WorkflowIcon className='h-4 w-4' /> },
  { value: "app", label: "Apps", icon: <AppWindow className='h-4 w-4' /> },
  {
    value: "automation",
    label: "Automations (legacy)",
    icon: <WorkflowIcon className='h-4 w-4' />
  },
  { value: "topic", label: "Topics", icon: <BookOpen className='h-4 w-4' /> },
  { value: "view", label: "Views", icon: <Eye className='h-4 w-4' /> },
  { value: "sql_query", label: "SQL Queries", icon: <FileCode className='h-4 w-4' /> },
  { value: "table", label: "Tables", icon: <Table className='h-4 w-4' /> },
  { value: "entity", label: "Entities", icon: <Box className='h-4 w-4' /> }
];

export const ROW_HEIGHT = 80;
export const MIN_NODE_WIDTH = 150;
export const PADDING = 40;
export const MAX_ROW_WIDTH = 1400;
