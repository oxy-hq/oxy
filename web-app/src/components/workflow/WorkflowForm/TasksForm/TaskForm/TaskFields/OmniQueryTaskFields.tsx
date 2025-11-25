import React from "react";
import { useFormContext, useFieldArray, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Plus, X } from "lucide-react";
import { WorkflowFormData } from "../..";

interface OmniQueryTaskFieldsProps {
  index: number;
  basePath?: string;
}

const ORDER_TYPES = [
  { value: "asc", label: "Ascending" },
  { value: "desc", label: "Descending" },
];

export const OmniQueryTaskFields: React.FC<OmniQueryTaskFieldsProps> = ({
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

  // Use useFieldArray for dynamic fields array
  const {
    fields: fieldEntries,
    append: appendField,
    remove: removeField,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.fields`,
  });

  // Use useFieldArray for sorts (key-value pairs: field name -> order type)
  const {
    fields: sortEntries,
    append: appendSort,
    remove: removeSort,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.sorts`,
  });

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.integration`}>Integration</Label>
        <Input
          id={`${taskPath}.integration`}
          placeholder="Enter integration name"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.integration`, {
            required: "Integration is required",
          })}
        />
        {taskErrors?.integration && (
          <p className="text-sm text-red-500">
            {taskErrors.integration.message}
          </p>
        )}
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.topic`}>Topic</Label>
        <Input
          id={`${taskPath}.topic`}
          placeholder="Enter topic"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.topic`, {
            required: "Topic is required",
          })}
        />
        {taskErrors?.topic && (
          <p className="text-sm text-red-500">{taskErrors.topic.message}</p>
        )}
      </div>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Fields</Label>
          <Button
            type="button"
            onClick={() => appendField("" as never)}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Field
          </Button>
        </div>
        {fieldEntries.length > 0 && (
          <div className="space-y-2">
            {fieldEntries.map((field, fieldIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Input
                    placeholder="view.field_name"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.fields.${fieldIndex}`,
                    )}
                  />
                </div>
                <Button
                  type="button"
                  onClick={() => removeField(fieldIndex)}
                  variant="ghost"
                  size="sm"
                >
                  <X className="w-4 h-4" />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className="text-sm text-muted-foreground">
          Add fields to select. Use format: view.field_name
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Sort Fields</Label>
          <Button
            type="button"
            onClick={() => appendSort({ field: "", order: "asc" } as never)}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Sort
          </Button>
        </div>
        {sortEntries.length > 0 && (
          <div className="space-y-2">
            {sortEntries.map((field, sortIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Input
                    placeholder="Field name"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.sorts.${sortIndex}.field`,
                    )}
                  />
                </div>
                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.sorts.${sortIndex}.order`}
                  render={({ field }) => (
                    <Select
                      value={field.value as string}
                      onValueChange={field.onChange}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {ORDER_TYPES.map((type) => (
                          <SelectItem key={type.value} value={type.value}>
                            {type.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />

                <Button
                  type="button"
                  onClick={() => removeSort(sortIndex)}
                  variant="ghost"
                  size="sm"
                >
                  <X className="w-4 h-4" />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className="text-sm text-muted-foreground">
          Add fields to sort by with direction (ascending/descending)
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.limit`}>Limit</Label>
        <Input
          id={`${taskPath}.limit`}
          type="number"
          min="0"
          placeholder="Optional limit"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.limit`, {
            valueAsNumber: true,
          })}
        />
      </div>
    </div>
  );
};
