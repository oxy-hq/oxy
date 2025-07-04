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
      result: string[][];
      is_result_truncated: boolean;
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

export type Artifact = SqlArtifact | AgentArtifact | WorkflowArtifact;
