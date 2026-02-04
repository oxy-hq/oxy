import { Plus } from "lucide-react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import type { WorkflowFormData } from "./index";
import { TaskForm } from "./TaskForm";

interface NestedTasksFormProps {
  /**
   * The field name for the tasks array
   * e.g., "tasks" for root level, "tasks.0.tasks" for nested
   */
  name: string;
  /**
   * Label for the tasks section
   */
  label?: React.ReactNode;
  /**
   * Minimum number of tasks required
   */
  minTasks?: number;
  /**
   * Show the add button
   */
  showAddButton?: boolean;
}

export const NestedTasksForm: React.FC<NestedTasksFormProps> = ({
  name,
  label,
  minTasks = 0,
  showAddButton = true
}) => {
  const { control } = useFormContext<WorkflowFormData>();

  const {
    fields: taskFields,
    append: appendTask,
    remove: removeTask
  } = useFieldArray({
    control,
    // @ts-expect-error - Dynamic field array path
    name
  });

  const handleRemoveTask = (index: number) => {
    if (taskFields.length > minTasks) {
      removeTask(index);
    }
  };

  return (
    <div className='space-y-4'>
      {showAddButton && (
        <div className='flex items-center justify-between'>
          {label}
          <Button
            type='button'
            onClick={() =>
              appendTask({
                name: `task_${taskFields.length + 1}`,
                type: "agent"
              })
            }
            variant='outline'
            size='sm'
          >
            <Plus className='mr-2 h-4 w-4' />
            Add Task
          </Button>
        </div>
      )}

      {taskFields.length === 0 && (
        <div className='rounded-lg border-2 border-muted-foreground/25 border-dashed p-6 text-center'>
          <p className='text-muted-foreground text-sm'>
            No tasks defined. Click "Add Task" to get started.
          </p>
        </div>
      )}

      <div className='space-y-4'>
        {taskFields.map((field, index) => (
          <div key={field.id}>
            <TaskForm index={index} onRemove={() => handleRemoveTask(index)} basePath={name} />
          </div>
        ))}
      </div>

      {minTasks > 0 && taskFields.length < minTasks && (
        <p className='text-amber-600 text-sm'>
          At least {minTasks} task{minTasks > 1 ? "s are" : " is"} required.
        </p>
      )}
    </div>
  );
};
