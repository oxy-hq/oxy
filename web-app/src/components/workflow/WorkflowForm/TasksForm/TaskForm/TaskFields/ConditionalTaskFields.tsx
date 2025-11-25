import React from "react";
import { useFormContext, useFieldArray } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Plus, X } from "lucide-react";
import { WorkflowFormData } from "../..";
import { NestedTasksForm } from "@/components/workflow/WorkflowForm/TasksForm/NestedTasksForm";

interface ConditionalTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const ConditionalTaskFields: React.FC<ConditionalTaskFieldsProps> = ({
  index,
  basePath = "tasks",
}) => {
  const {
    register,
    control,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // Use useFieldArray for conditions array
  const {
    fields: conditionFields,
    append: appendCondition,
    remove: removeCondition,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.conditions`,
  });

  return (
    <div className="space-y-4">
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <Label className="text-base font-semibold">Conditions</Label>
          <Button
            type="button"
            onClick={() => appendCondition({ if_expr: "", tasks: [] } as never)}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Condition
          </Button>
        </div>

        {conditionFields.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No conditions defined. Add at least one condition.
          </p>
        )}

        {conditionFields.map((field, conditionIndex) => (
          <div
            key={field.id}
            className="space-y-4 p-4 border rounded-lg bg-muted/30"
          >
            <div className="flex items-center justify-between">
              <CardTitle className="text-sm">
                Condition {conditionIndex + 1}
              </CardTitle>
              <Button
                type="button"
                onClick={() => removeCondition(conditionIndex)}
                variant="ghost"
                size="sm"
              >
                <X className="w-4 h-4 text-destructive" />
              </Button>
            </div>

            <div className="space-y-2">
              <Label htmlFor={`${taskPath}.conditions.${conditionIndex}.if`}>
                If Expression
              </Label>
              <Input
                id={`${taskPath}.conditions.${conditionIndex}.if`}
                placeholder="e.g., {{ task_name.result }} == 'success'"
                {...register(
                  // @ts-expect-error - dynamic field path
                  `${taskPath}.conditions.${conditionIndex}.if`,
                  {
                    required: "Condition expression is required",
                  },
                )}
              />
              {taskErrors?.conditions?.[conditionIndex]?.if && (
                <p className="text-sm text-red-500">
                  {taskErrors.conditions[conditionIndex].if.message}
                </p>
              )}
              <p className="text-xs text-muted-foreground">
                Use template syntax to reference task outputs (e.g.,{" "}
                {`{{ task_name.field }}`})
              </p>
            </div>

            {/* Nested tasks for this condition */}
            <div className="space-y-2 pt-2 border-t">
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
          <p className="text-sm text-red-500">
            {taskErrors.conditions.message}
          </p>
        )}
      </div>

      {/* Else tasks (optional) */}
      <div className="space-y-4 pt-4 border-t">
        <NestedTasksForm
          label={<Label>Else Tasks (optional)</Label>}
          name={`${taskPath}.else`}
          minTasks={0}
          showAddButton={true}
        />
        <p className="text-xs text-muted-foreground">
          These tasks will execute if none of the conditions above are true
        </p>
      </div>
    </div>
  );
};
