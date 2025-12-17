export interface OntologyNode {
  id: string;
  type:
    | "table"
    | "view"
    | "topic"
    | "workflow"
    | "app"
    | "automation"
    | "sql_query"
    | "agent"
    | "entity";
  label: string;
  data: {
    name: string;
    path?: string;
    description?: string;
    database?: string;
    datasource?: string;
    metadata?: Record<string, unknown>;
  };
}

export interface OntologyEdge {
  id: string;
  source: string;
  target: string;
  label?: string;
  type?: "references" | "uses" | "contains" | "derived_from";
}

export interface OntologyGraph {
  nodes: OntologyNode[];
  edges: OntologyEdge[];
}

export interface View {
  name: string;
  path: string;
  description?: string;
  datasource: string;
  table: string;
  entities?: Array<{
    name: string;
    type: string;
    description?: string;
    keys?: string[];
  }>;
  dimensions?: Array<{
    name: string;
    type: string;
    sql?: string;
  }>;
  measures?: Array<{
    name: string;
    type: string;
    sql?: string;
  }>;
}

export interface Topic {
  name: string;
  path: string;
  description?: string;
  views: string[];
  base_view?: string;
  default_filters?: Array<{
    field: string;
    op: string;
    value: unknown;
  }>;
}
