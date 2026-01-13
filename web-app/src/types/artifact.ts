import { LogItem } from "@/services/types";

export type SqlArtifact = {
  id: string;
  name: string;
  kind: "execute_sql";
  is_streaming?: boolean;
  content: {
    type: "execute_sql";
    value: {
      database: string;
      sql_query: string;
      result?: string[][];
      result_file?: string;
      is_result_truncated: boolean;
    };
  };
};

export type SemanticQueryArtifact = {
  id: string;
  name: string;
  kind: "semantic_query";
  is_streaming?: boolean;
  content: {
    type: "semantic_query";
    value: {
      error?: string;
      validation_error?: string;
      sql_generation_error?: string;
      database: string;
      sql_query: string;
      result?: string[][];
      result_file?: string;
      is_result_truncated: boolean;
      topic: string;
      dimensions: string[];
      measures: string[];
      filters: Array<{
        field: string;
        op: string;
        value: string | number | boolean | string[] | number[];
      }>;
      orders: Array<{
        field: string;
        direction: string;
      }>;
      limit?: number;
      offset?: number;
    };
  };
};

export type OmniQueryArtifact = {
  id: string;
  name: string;
  kind: "omni_query";
  is_streaming?: boolean;
  content: {
    type: "omni_query";
    value: {
      database: string;
      sql: string;
      result?: string[][];
      result_file?: string;
      is_result_truncated: boolean;
      fields: string[];
      limit?: number;
      sorts?: Record<string, "asc" | "desc">;
    };
  };
};

export type AgentArtifact = {
  id: string;
  kind: "agent";
  name: string;
  is_streaming?: boolean;
  content: {
    type: "agent";
    value: {
      ref: string;
      output: string;
    };
  };
};

export type WorkflowArtifact = {
  id: string;
  name: string;
  kind: "workflow";
  is_streaming?: boolean;
  content: {
    type: "workflow";
    value: {
      ref: string;
      output: LogItem[];
    };
  };
};

export type Artifact =
  | SqlArtifact
  | SemanticQueryArtifact
  | OmniQueryArtifact
  | AgentArtifact
  | WorkflowArtifact;
