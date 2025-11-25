import { CardTitle } from "@/components/ui/shadcn/card";
import { NestedTasksForm } from "./NestedTasksForm";

export interface WorkflowFormData {
  name?: string;
  description?: string;
  tasks?: TaskFormData[];
  variables?: string;
  tests?: TestFormData[];
  retrieval?: RetrievalConfigData | null;
}

export interface TaskFormData {
  name?: string;
  type?: string;
  cache?: {
    enabled?: boolean;
    path?: string;
  };
  export?: {
    enabled?: boolean;
    format?: string;
    path?: string;
  };
  [key: string]: unknown;
}

export interface TestFormData {
  type?: string;
  concurrency?: number;
  task_ref?: string;
  metrics?: unknown[];
  [key: string]: unknown;
}

export interface RetrievalConfigData {
  include?: string[];
  exclude?: string[];
}

export const TasksForm = () => {
  return (
    <>
      <NestedTasksForm
        label={<CardTitle>Tasks</CardTitle>}
        name="tasks"
        showAddButton={true}
      />
    </>
  );
};
