import React, { useEffect } from "react";
import { useForm, FormProvider, useFieldArray } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Plus } from "lucide-react";
import { VariablesForm } from "./VariablesForm";
import { TestsForm } from "@/components/shared/TestsForm";
import { RetrievalForm } from "./RetrievalForm";
import { Input } from "@/components/ui/shadcn/input";
import { TasksForm } from "./TasksForm";
import { cleanObject } from "@/utils/formDataCleaner";

export interface WorkflowFormData {
  name?: string;
  description?: string;
  tasks?: TaskFormData[];
  variables?: unknown;
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

interface WorkflowFormProps {
  data?: Partial<WorkflowFormData>;
  onChange?: (data: Partial<WorkflowFormData>) => void;
}

const cleanFormData = (
  data: Partial<WorkflowFormData>,
): Partial<WorkflowFormData> => {
  return (
    (cleanObject(
      data as Record<string, unknown>,
    ) as Partial<WorkflowFormData>) || {}
  );
};

const getDefaultData = (data?: Partial<WorkflowFormData>) => {
  if (!data) {
    return {
      name: "",
      description: "",
      tasks: [{ name: "task_1", type: "agent" }],
      variables: "{}",
      tests: [],
      retrieval: null,
    };
  }

  const result: Partial<WorkflowFormData> = {};

  if (data.name !== undefined) result.name = data.name;
  if (data.description !== undefined) result.description = data.description;
  if (data.variables !== undefined) result.variables = data.variables;

  if (data.tasks && Array.isArray(data.tasks) && data.tasks.length > 0) {
    result.tasks = data.tasks.map((task) => {
      const processedTask = { ...task };

      if (processedTask.export && typeof processedTask.export === "object") {
        if (processedTask.export.format || processedTask.export.path) {
          processedTask.export.enabled = true;
        }
      }

      return processedTask;
    });
  }

  if (data.tests && Array.isArray(data.tests) && data.tests.length > 0) {
    result.tests = data.tests;
  }

  if (data.retrieval && typeof data.retrieval === "object") {
    const hasInclude =
      data.retrieval.include &&
      Array.isArray(data.retrieval.include) &&
      data.retrieval.include.length > 0;
    const hasExclude =
      data.retrieval.exclude &&
      Array.isArray(data.retrieval.exclude) &&
      data.retrieval.exclude.length > 0;
    if (hasInclude || hasExclude) {
      result.retrieval = data.retrieval;
    }
  }

  console.log("Processed initial data:", result);
  return result;
};

export const WorkflowForm: React.FC<WorkflowFormProps> = ({
  data,
  onChange,
}) => {
  const methods = useForm<WorkflowFormData>({
    defaultValues: getDefaultData(data),
    mode: "onBlur",
  });

  const { watch } = methods;

  useEffect(() => {
    const subscription = watch((value) => {
      if (methods.formState.isDirty) {
        const cleaned = cleanFormData(value as Partial<WorkflowFormData>);
        onChange?.(cleaned);
      }
    });
    return () => subscription.unsubscribe();
  }, [watch, onChange]);

  const {
    control,
    register,
    formState: { errors },
  } = methods;

  const {
    fields: testFields,
    append: appendTest,
    remove: removeTest,
  } = useFieldArray({
    control,
    name: "tests",
  });

  return (
    <FormProvider {...methods}>
      <div className="flex-1 min-h-0 flex flex-col bg-card">
        <div className="flex-1 overflow-auto customScrollbar p-6">
          <form id="workflow-form" className="space-y-8">
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="name">Name</Label>
                <Input
                  id="name"
                  placeholder="Describe what this automation does..."
                  {...register("name")}
                />
                {errors.name && (
                  <p className="text-sm text-red-500">{errors.name.message}</p>
                )}
              </div>
              <div className="space-y-2">
                <Label htmlFor="description">Description</Label>
                <Textarea
                  id="description"
                  placeholder="Describe what this automation does..."
                  {...register("description")}
                  rows={4}
                />
                {errors.description && (
                  <p className="text-sm text-red-500">
                    {errors.description.message}
                  </p>
                )}
              </div>
            </div>

            <TasksForm />

            <VariablesForm />

            <div className="flex items-center justify-between">
              <CardTitle>Tests</CardTitle>
              <Button
                type="button"
                onClick={() =>
                  appendTest({
                    type: "consistency",
                    concurrency: 10,
                  })
                }
                variant="outline"
                size="sm"
              >
                <Plus className="w-4 h-4 mr-2" />
                Add Test
              </Button>
            </div>
            <div className="space-y-4">
              {testFields.map((field, index) => (
                <TestsForm<WorkflowFormData>
                  key={field.id}
                  index={index}
                  onRemove={() => removeTest(index)}
                />
              ))}
            </div>

            <RetrievalForm />
          </form>
        </div>
      </div>
    </FormProvider>
  );
};
