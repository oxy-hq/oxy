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

export interface Message {
  content: string;
  references: Reference[];
  steps: string[];
  isUser: boolean;
  isStreaming: boolean;
}
