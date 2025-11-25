import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { WorkflowFormData } from "../..";
import { NestedTasksForm } from "@/components/workflow/WorkflowForm/TasksForm/NestedTasksForm";

interface LoopSequentialTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const LoopSequentialTaskFields: React.FC<
  LoopSequentialTaskFieldsProps
> = ({ index, basePath = "tasks" }) => {
  const {
    register,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.values`}>Values</Label>
        <Textarea
          id={`${taskPath}.values`}
          placeholder='Enter values as JSON array (e.g., ["item1", "item2"]) or template string (e.g., {{ task_output }})'
          rows={3}
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.values`, {
            required: "Values are required",
          })}
        />
        {taskErrors?.values && (
          <p className="text-sm text-red-500">{taskErrors.values.message}</p>
        )}
        <p className="text-xs text-muted-foreground">
          Can be a JSON array or a template string referencing a task output
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.concurrency`}>Concurrency</Label>
        <Input
          id={`${taskPath}.concurrency`}
          type="number"
          min="1"
          defaultValue={1}
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.concurrency`, {
            valueAsNumber: true,
          })}
        />
        <p className="text-xs text-muted-foreground">
          Number of tasks to run concurrently
        </p>
      </div>

      {/* Nested Tasks */}
      <div className="space-y-4 pt-4 border-t">
        <NestedTasksForm
          name={`${taskPath}.tasks`}
          label={<Label>Tasks to execute for each value</Label>}
          minTasks={1}
          showAddButton={true}
        />
        <p className="text-xs text-muted-foreground">
          These tasks will be executed for each value in the loop. You can
          reference the current loop value in task prompts.
        </p>
      </div>
    </div>
  );
};
