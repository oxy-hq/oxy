export interface AnonymizerConfigFlashText {
  type: "flash_text";
  case_sensitive?: boolean;
  pluralize?: boolean;
  keywords_file?: string;
  replacement?: string;
  mapping_file?: string;
  delimiter?: string;
}

export type AnonymizerConfig = AnonymizerConfigFlashText;

export interface AgentContextFile {
  type: "file";
  name: string;
  src: string[];
}

export interface AgentContextSemanticModel {
  type: "semantic_model";
  name: string;
  src: string;
}

export type AgentContext = AgentContextFile | AgentContextSemanticModel;

type OutputFormat = "default" | "file";

export interface ExecuteSqlTool {
  type: "execute_sql";
  name: string;
  database: string;
  description?: string; // default: "Execute the SQL query. If the query is invalid, fix it and run again."
}

export interface ValidateSqlTool {
  type: "validate_sql";
  name: string;
  database: string;
  description?: string; // default: "Validate the SQL query. If the query is invalid, fix it and run again."
}

export interface RetrievalTool {
  type: "retrieval";
  name: string;
  src: string[];
  api_key?: string | null;
  api_url?: string; // default: "https://api.openai.com/v1"
  description?: string; // default: "Retrieve the relevant SQL queries to support query generation."
  embed_model?: string; // default: "text-embedding-3-small"
  factor?: number; // default: 5
  key_var?: string; // default: "OPENAI_API_KEY"
  n_dims?: number; // default: 512
  top_k?: number; // default: 4
}

export type ToolConfig = ExecuteSqlTool | ValidateSqlTool | RetrievalTool;

export interface AgentConfig {
  model: string;
  system_instructions: string;
  anonymize?: AnonymizerConfig | null;
  context?: AgentContext[] | null;
  output_format?: OutputFormat;
  tools?: ToolConfig[];
}
