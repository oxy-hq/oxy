export interface AgentConfig {
  name: string;
  model: string;
  system_instructions: string;
  tools: ToolType[];
  context?: AgentContext[];
  output_format: OutputFormat;
  anonymize?: AnonymizerConfig;
  tests: Eval[];
  description: string;
}

type ToolType = ExecuteSQLTool | ValidateSQLTool | RetrievalConfig;

interface ExecuteSQLTool {
  type: "execute_sql";
  name: string;
  description: string;
  database: string;
}

interface ValidateSQLTool {
  type: "validate_sql";
  name: string;
  description: string;
  database: string;
}

interface RetrievalConfig {
  type: "retrieval";
  name: string;
  description: string;
  src: string[];
  embed_model: string;
  api_url: string;
  api_key?: string;
  key_var: string;
  n_dims: number;
  top_k: number;
  factor: number;
}

interface AgentContext {
  name: string;
  contextType: AgentContextType;
}

type AgentContextType = FileContext | SemanticModelContext;

interface FileContext {
  type: "file";
  src: string[];
}

interface SemanticModelContext {
  type: "semantic_model";
  src: string;
}

enum OutputFormat {
  Default = "default",
  File = "file",
}

interface AnonymizerConfig {
  type: "flash_text";
  source: FlashTextSourceType;
  pluralize: boolean;
  case_sensitive: boolean;
}

type FlashTextSourceType = KeywordsSource | MappingSource;

interface KeywordsSource {
  keywords_file: string;
  replacement: string;
}

interface MappingSource {
  mapping_file: string;
  delimiter: string;
}

export type Eval = ConsistencyEval;

interface ConsistencyEval {
  type: "consistency";
  prompt: string;
  model_ref?: string;
  n: number;
  task_description?: string;
  task_ref?: string;
  scores: Record<string, number>;
  concurrency: number;
}

export const STEP_MAP = {
  execute_sql: "Execute SQL",
  visualize: "Generate visualization",
  retrieve: "Retrieve data",
};
