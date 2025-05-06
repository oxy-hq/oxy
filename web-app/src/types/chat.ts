export type Answer = {
  content: string;
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
  question: string;
  answer: string;
  agent: string;
  created_at: string;
  references: Reference[];
};

export type ThreadCreateRequest = {
  title: string;
  question: string;
  agent: string;
};

export type TaskItem = {
  id: string;
  title: string;
  question: string;
  answer: string;
  file_path: string;
  created_at: string;
};

export type TaskCreateRequest = {
  title: string;
  question: string;
};
export interface Message {
  content: string;
  references: Reference[];
  steps: string[];
  isUser: boolean;
  isStreaming: boolean;
}
