export interface Workflow {
  name: string;
  tasks: unknown[];
  tests?: unknown[];
  variables?: Record<string, unknown>;
  description?: string;
  path: string;
}
