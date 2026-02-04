import { Plus } from "lucide-react";
import type React from "react";
import { useEffect } from "react";
import { FormProvider, useFieldArray, useForm } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import { NestedTasksForm } from "@/components/workflow/WorkflowForm/TasksForm/NestedTasksForm";
import { cleanObject } from "@/utils/formDataCleaner";
import { DisplayForm } from "./DisplayForm";

export interface AppFormData {
  tasks?: TaskFormData[];
  display?: DisplayFormData[];
}

export interface TaskFormData {
  name?: string;
  type?: string;
  cache?: {
    enabled?: boolean;
    path?: string;
  };
  export?: {
    format?: string;
    path?: string;
  };
  [key: string]: unknown;
}

export interface DisplayFormData {
  type?: string;
  [key: string]: unknown;
}

interface AppFormProps {
  data?: Partial<AppFormData>;
  onChange?: (data: Partial<AppFormData>) => void;
}

const cleanFormData = (data: Partial<AppFormData>): Partial<AppFormData> => {
  const cleaned: Partial<AppFormData> = {};

  if (data.tasks && Array.isArray(data.tasks) && data.tasks.length > 0) {
    const cleanedTasks = data.tasks
      .map(cleanObject)
      .filter((task): task is TaskFormData => task !== null);
    if (cleanedTasks.length > 0) {
      cleaned.tasks = cleanedTasks;
    }
  }

  if (data.display && Array.isArray(data.display) && data.display.length > 0) {
    const cleanedDisplay = data.display
      .map(cleanObject)
      .filter((display): display is DisplayFormData => display !== null);
    if (cleanedDisplay.length > 0) {
      cleaned.display = cleanedDisplay;
    }
  }

  return cleaned;
};

const getDefaultData = (data?: Partial<AppFormData>) => {
  if (!data) {
    return {
      tasks: [{ name: "task_1", type: "execute_sql" }],
      display: [{ type: "table" }]
    };
  }

  const result: Partial<AppFormData> = {};

  if (data.tasks && Array.isArray(data.tasks) && data.tasks.length > 0) {
    result.tasks = data.tasks;
  }

  if (data.display && Array.isArray(data.display) && data.display.length > 0) {
    result.display = data.display;
  }

  return result;
};

export const AppForm: React.FC<AppFormProps> = ({ data, onChange }) => {
  const methods = useForm<AppFormData>({
    defaultValues: getDefaultData(data),
    mode: "onBlur"
  });

  useEffect(() => {
    const subscription = methods.watch((value) => {
      if (methods.formState.isDirty) {
        const cleaned = cleanFormData(value as Partial<AppFormData>);
        onChange?.(cleaned);
      }
    });
    return () => subscription.unsubscribe();
  }, [methods, onChange]);

  const { control } = methods;

  const {
    fields: displayFields,
    append: appendDisplay,
    remove: removeDisplay
  } = useFieldArray({
    control,
    name: "display"
  });

  return (
    <FormProvider {...methods}>
      <div className='flex min-h-0 flex-1 flex-col bg-card'>
        <div className='customScrollbar flex-1 overflow-auto p-6'>
          <form id='app-form' className='space-y-8'>
            <NestedTasksForm
              label={<CardTitle>Tasks</CardTitle>}
              name='tasks'
              showAddButton={true}
            />

            <div className='flex items-center justify-between'>
              <CardTitle>Display</CardTitle>
              <Button
                type='button'
                onClick={() =>
                  appendDisplay({
                    type: "table"
                  })
                }
                variant='outline'
                size='sm'
              >
                <Plus className='mr-2 h-4 w-4' />
                Add Display
              </Button>
            </div>
            <div className='space-y-4'>
              {displayFields.map((field, index) => (
                <div key={field.id}>
                  <DisplayForm index={index} onRemove={() => removeDisplay(index)} />
                </div>
              ))}
            </div>
          </form>
        </div>
      </div>
    </FormProvider>
  );
};
