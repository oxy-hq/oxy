export interface ModelingProjectInfo {
  name: string;
  folder_name?: string;
  project_dir: string;
  model_paths: string[];
  seed_paths: string[];
}

export interface NodeColumnDef {
  name: string;
  description: string | null;
  data_type: string | null;
}

export interface NodeSummary {
  unique_id: string;
  name: string;
  resource_type: string;
  path: string;
  materialization: string | null;
  description: string | null;
  depends_on: string[];
  tags: string[];
  raw_sql: string | null;
  compiled_sql: string | null;
  columns: NodeColumnDef[];
  database: string | null;
  schema: string | null;
}

export interface CompileErrorEntry {
  node_id: string;
  message: string;
}

export interface CompiledNodeInfo {
  unique_id: string;
  name: string;
  compiled_sql: string;
}

export interface CompileOutput {
  models_compiled: number;
  errors: CompileErrorEntry[];
  nodes: CompiledNodeInfo[];
}

export interface NodeRunResult {
  unique_id: string;
  name: string;
  status: string;
  duration_ms: number;
  rows_affected: number | null;
  message: string | null;
}

export interface RunOutput {
  status: string;
  results: NodeRunResult[];
  duration_ms: number;
}

export interface TestResultEntry {
  test_name: string;
  model_name: string;
  column_name: string;
  status: string;
  failures: number;
  duration_ms: number;
  message: string | null;
}

export interface TestOutput {
  tests_run: number;
  passed: number;
  failed: number;
  results: TestResultEntry[];
}

export interface DiagnosticEntry {
  kind: string;
  message: string;
}

export interface ContractViolationEntry {
  model: string;
  kind: string;
  message: string;
}

export interface ColumnInfo {
  name: string;
  data_type: string;
  nullable: boolean;
}

export interface SchemaEntry {
  name: string;
  columns: ColumnInfo[];
}

export interface AnalyzeOutput {
  models_analyzed: number;
  cached_count: number;
  diagnostics: DiagnosticEntry[];
  contract_violations: ContractViolationEntry[];
  schemas: SchemaEntry[];
}

export interface LineageNode {
  unique_id: string;
  name: string;
  resource_type: string;
  description: string | null;
  path: string | null;
}

export interface LineageEdge {
  source: string;
  target: string;
}

export interface LineageOutput {
  nodes: LineageNode[];
  edges: LineageEdge[];
}

export interface ColumnLineageEntry {
  source_node: string;
  source_column: string;
  target_node: string;
  target_column: string;
  dependency_type: string;
}

export interface ColumnLineageOutput {
  edges: ColumnLineageEntry[];
}

export interface SeedOutput {
  seeds_loaded: number;
  results: NodeRunResult[];
}

export interface RunRequest {
  selector?: string;
  full_refresh?: boolean;
}

export type RunStreamEvent =
  | { kind: "node_started"; unique_id: string; name: string }
  | ({ kind: "node_completed" } & NodeRunResult)
  | { kind: "done"; status: string; duration_ms: number }
  | { kind: "error"; message: string };
