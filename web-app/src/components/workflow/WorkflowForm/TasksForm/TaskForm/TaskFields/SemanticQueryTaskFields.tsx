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

interface SemanticQueryTaskFieldsProps {
  index: number;
  basePath?: string;
}

const FILTER_OPERATORS = [
  { value: "eq", label: "Equals (=)" },
  { value: "neq", label: "Not Equals (≠)" },
  { value: "gt", label: "Greater Than (>)" },
  { value: "gte", label: "Greater Than or Equal (≥)" },
  { value: "lt", label: "Less Than (<)" },
  { value: "lte", label: "Less Than or Equal (≤)" },
  { value: "in", label: "In (array)" },
  { value: "not_in", label: "Not In (array)" },
];

const ORDER_DIRECTIONS = [
  { value: "asc", label: "Ascending" },
  { value: "desc", label: "Descending" },
];

export const SemanticQueryTaskFields: React.FC<
  SemanticQueryTaskFieldsProps
> = ({ index, basePath = "tasks" }) => {
  const {
    register,
    control,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // Use useFieldArray for dynamic arrays
  const {
    fields: dimensionFields,
    append: appendDimension,
    remove: removeDimension,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.dimensions`,
  });

  const {
    fields: measureFields,
    append: appendMeasure,
    remove: removeMeasure,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.measures`,
  });

  const {
    fields: filterFields,
    append: appendFilter,
    remove: removeFilter,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.filters`,
  });

  const {
    fields: orderFields,
    append: appendOrder,
    remove: removeOrder,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.orders`,
  });

  return (
    <div className="space-y-4">
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
          <Label>Dimensions</Label>
          <Button
            type="button"
            onClick={() => appendDimension("" as never)}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Dimension
          </Button>
        </div>
        {dimensionFields.length > 0 && (
          <div className="space-y-2">
            {dimensionFields.map((field, dimIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Input
                    placeholder="view_name.dimension_name"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.dimensions.${dimIndex}`,
                    )}
                  />
                </div>
                <Button
                  type="button"
                  onClick={() => removeDimension(dimIndex)}
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
          Add dimensions to include in the query (e.g., users.country,
          orders.status)
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Measures</Label>
          <Button
            type="button"
            onClick={() => appendMeasure("" as never)}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Measure
          </Button>
        </div>
        {measureFields.length > 0 && (
          <div className="space-y-2">
            {measureFields.map((field, measureIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Input
                    placeholder="view_name.measure_name"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.measures.${measureIndex}`,
                    )}
                  />
                </div>
                <Button
                  type="button"
                  onClick={() => removeMeasure(measureIndex)}
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
          Add measures to aggregate in the query (e.g., orders.total_revenue,
          users.count)
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Filters</Label>
          <Button
            type="button"
            onClick={() =>
              appendFilter({ field: "", op: "eq", value: "" } as never)
            }
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Filter
          </Button>
        </div>
        {filterFields.length > 0 && (
          <div className="space-y-2">
            {filterFields.map((field, filterIndex) => (
              <div key={field.id} className="flex gap-2 items-center">
                <div className="flex-1">
                  <Input
                    placeholder="Field (e.g., view.dimension)"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.filters.${filterIndex}.field`,
                    )}
                  />
                </div>
                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.filters.${filterIndex}.op`}
                  render={({ field }) => (
                    <Select
                      value={field.value as string}
                      onValueChange={field.onChange}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {FILTER_OPERATORS.map((op) => (
                          <SelectItem key={op.value} value={op.value}>
                            {op.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />

                <div className="flex-1">
                  <Input
                    placeholder="Value (JSON format)"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.filters.${filterIndex}.value`,
                    )}
                  />
                </div>
                <Button
                  type="button"
                  onClick={() => removeFilter(filterIndex)}
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
          Add filters to narrow down query results. Value should be JSON format
          (e.g., "value" or ["val1", "val2"])
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Order By</Label>
          <Button
            type="button"
            onClick={() =>
              appendOrder({ field: "", direction: "asc" } as never)
            }
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Order
          </Button>
        </div>
        {orderFields.length > 0 && (
          <div className="space-y-2">
            {orderFields.map((field, orderIndex) => (
              <div key={field.id} className="flex gap-2 items-center">
                <div className="flex-1">
                  <Input
                    placeholder="Field (e.g., view.dimension)"
                    {...register(
                      // @ts-expect-error - dynamic field path
                      `${taskPath}.orders.${orderIndex}.field`,
                    )}
                  />
                </div>

                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.orders.${orderIndex}.direction`}
                  render={({ field }) => (
                    <Select
                      value={field.value as string}
                      onValueChange={field.onChange}
                    >
                      <SelectTrigger>
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        {ORDER_DIRECTIONS.map((dir) => (
                          <SelectItem key={dir.value} value={dir.value}>
                            {dir.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />

                <Button
                  type="button"
                  onClick={() => removeOrder(orderIndex)}
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
          Specify how to sort the query results
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
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
        <div className="space-y-2">
          <Label htmlFor={`${taskPath}.offset`}>Offset</Label>
          <Input
            id={`${taskPath}.offset`}
            type="number"
            min="0"
            placeholder="Optional offset"
            // @ts-expect-error - dynamic field path
            {...register(`${taskPath}.offset`, {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>
    </div>
  );
};
