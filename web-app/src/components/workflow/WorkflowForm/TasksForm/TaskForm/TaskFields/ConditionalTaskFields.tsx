import { Plus, X } from "lucide-react";
import type React from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { NestedTasksForm } from "@/components/workflow/WorkflowForm/TasksForm/NestedTasksForm";
import type { WorkflowFormData } from "../..";

interface ConditionalTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const ConditionalTaskFields: React.FC<ConditionalTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
  const {
    register,
    control,
    formState: { errors }
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // Use useFieldArray for conditions array
  const {
    fields: conditionFields,
    append: appendCondition,
    remove: removeCondition
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.conditions`
  });

  return (
    <div className='space-y-4'>
      <div className='space-y-4'>
        <div className='flex items-center justify-between'>
          <Label className='font-semibold text-base'>Conditions</Label>
          <Button
            type='button'
            onClick={() => appendCondition({ if_expr: "", tasks: [] } as never)}
            variant='outline'
            size='sm'
          >
            <Plus className='mr-1 h-4 w-4' />
            Add Condition
          </Button>
        </div>

        {conditionFields.length === 0 && (
          <p className='text-muted-foreground text-sm'>
            No conditions defined. Add at least one condition.
          </p>
        )}

        {conditionFields.map((field, conditionIndex) => (
          <div key={field.id} className='space-y-4 rounded-lg border bg-muted/30 p-4'>
            <div className='flex items-center justify-between'>
              <CardTitle className='text-sm'>Condition {conditionIndex + 1}</CardTitle>
              <Button
                type='button'
                onClick={() => removeCondition(conditionIndex)}
                variant='ghost'
                size='sm'
              >
                <X className='h-4 w-4 text-destructive' />
              </Button>
            </div>

            <div className='space-y-2'>
              <Label htmlFor={`${taskPath}.conditions.${conditionIndex}.if`}>If Expression</Label>
              <Input
                id={`${taskPath}.conditions.${conditionIndex}.if`}
                placeholder="e.g., {{ task_name.result }} == 'success'"
                {...register(
                  // @ts-expect-error - dynamic field path
                  `${taskPath}.conditions.${conditionIndex}.if`,
                  {
                    required: "Condition expression is required"
                  }
                )}
              />
              {taskErrors?.conditions?.[conditionIndex]?.if && (
                <p className='text-red-500 text-sm'>
                  {taskErrors.conditions[conditionIndex].if.message}
                </p>
              )}
              <p className='text-muted-foreground text-xs'>
                Use template syntax to reference task outputs (e.g., {`{{ task_name.field }}`})
              </p>
            </div>

            {/* Nested tasks for this condition */}
            <div className='space-y-2 border-t pt-2'>
              <NestedTasksForm
                name={`${taskPath}.conditions.${conditionIndex}.tasks`}
                label={<Label>Tasks to execute when condition is true</Label>}
                minTasks={1}
                showAddButton={true}
              />
            </div>
          </div>
        ))}

        {taskErrors?.conditions && !Array.isArray(taskErrors.conditions) && (
          <p className='text-red-500 text-sm'>{taskErrors.conditions.message}</p>
        )}
      </div>

      {/* Else tasks (optional) */}
      <div className='space-y-4 border-t pt-4'>
        <NestedTasksForm
          label={<Label>Else Tasks (optional)</Label>}
          name={`${taskPath}.else`}
          minTasks={0}
          showAddButton={true}
        />
        <p className='text-muted-foreground text-xs'>
          These tasks will execute if none of the conditions above are true
        </p>
      </div>
    </div>
  );
};
