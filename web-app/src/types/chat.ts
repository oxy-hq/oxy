import { LogItem } from "@/services/types";

export interface TextContent {
  type: "text";
  content: string;
}

export interface WorkflowArtifactKind {
  type: "workflow";
  value: {
    ref: string;
  };
}

export interface AgentArtifactKind {
  type: "agent";
  value: {
    ref: string;
  };
}

export interface ExecuteSQLArtifactKind {
  type: "execute_sql";
  value: {
    database: string;
  };
}

export type ArtifactKind =
  | WorkflowArtifactKind
  | AgentArtifactKind
  | ExecuteSQLArtifactKind;

export interface ArtifactStartedContent {
  type: "artifact_started";
  id: string;
  title: string;
  kind: ArtifactKind;
}

export interface WorkflowArtifactValue {
  type: "log_item";
  value: LogItem;
}

export interface AgentArtifactValue {
  type: "content";
  value: string;
}

export interface ExecuteSQLArtifactValue {
  type: "execute_sql";
  value: {
    database: string;
    sql_query: string;
    result: string[][];
    is_result_truncated: boolean;
  };
}

export type ArtifactValue =
  | WorkflowArtifactValue
  | AgentArtifactValue
  | ExecuteSQLArtifactValue;

export interface ArtifactValueContent {
  type: "artifact_value";
  id: string;
  value: ArtifactValue;
}

export interface ArtifactDoneContent {
  type: "artifact_done";
  id: string;
}

export interface ErrorContent {
  type: "error";
  message: string;
}

export interface UsageContent {
  type: "usage";
  usage: Usage;
}

export type AnswerContent =
  | TextContent
  | ArtifactStartedContent
  | ArtifactValueContent
  | ArtifactDoneContent
  | UsageContent
  | ErrorContent;

export type Answer = {
  content: AnswerContent;
  references: Reference[];
  step: string;
  is_error: boolean;
};

export type ToolCallMetadata = SqlQueryReference | { type: ReferenceType };

export type Reference = (SqlQueryReference | DataAppReference) & {
  type: ReferenceType;
};

export enum ReferenceType {
  SQLQuery = "sqlQuery",
  DataApp = "dataApp",
}

export type SqlQueryReference = {
  type: ReferenceType.SQLQuery;
  database: string;
  sql_query: string;
  result: string[][];
  is_result_truncated: boolean;
};

export type DataAppReference = {
  type: ReferenceType.DataApp;
  file_path: string;
};

export type ThreadItem = {
  id: string;
  title: string;
  input: string;
  output: string;
  source: string;
  source_type: string;
  created_at: string;
  references: Reference[];
};

export type ThreadCreateRequest = {
  title: string;
  input: string;
  source: string;
  source_type: string;
};

export type PaginationInfo = {
  page: number;
  limit: number;
  total: number;
  total_pages: number;
  has_next: boolean;
  has_previous: boolean;
};

export type ThreadsResponse = {
  threads: ThreadItem[];
  pagination: PaginationInfo;
};

export type Usage = {
  inputTokens: number;
  outputTokens: number;
};

export interface Message {
  content: string;
  references: Reference[];
  steps: string[];
  isUser: boolean;
  isStreaming: boolean;
  usage: Usage;
}

export interface MessageItem {
  id: string;
  content: string;
  is_human: boolean;
  thread_id: string;
  created_at: string;
  usage: Usage;
}
