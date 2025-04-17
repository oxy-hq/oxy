export type Answer = {
  content: string;
  references: Reference[];
  step: string;
  is_error: boolean;
};

export type ToolCallMetadata = SqlQueryReference | { type: ReferenceType };

export type Reference = SqlQueryReference & { type: ReferenceType };

export enum ReferenceType {
  SQLQuery = "sqlQuery",
}

export type SqlQueryReference = {
  type: ReferenceType.SQLQuery;
  database: string;
  sql_query: string;
  result: string[][];
  is_result_truncated: boolean;
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
