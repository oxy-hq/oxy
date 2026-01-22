import React, { useEffect, useRef } from "react";
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
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Plus, X, Loader2 } from "lucide-react";
import { WorkflowFormData } from "../..";
import useTopicFiles from "@/hooks/api/useTopicFiles";
import useTopicFieldOptions from "@/hooks/api/useTopicFieldOptions";

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
    watch,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // Watch the topic value
  // @ts-expect-error - dynamic field path
  const topicValue = watch(`${taskPath}.topic`) as string | undefined;

  // Track previous topic to detect changes
  const prevTopicRef = useRef<string | undefined>(topicValue);

  // Fetch available topics
  const {
    topicFiles,
    isLoading: topicsLoading,
    error: topicsError,
  } = useTopicFiles();

  // Fetch field options based on selected topic
  const {
    dimensions: dimensionOptions,
    measures: measureOptions,
    allFields: allFieldOptions,
    isLoading: fieldsLoading,
  } = useTopicFieldOptions(topicValue);

  // Use useFieldArray for dynamic arrays
  const {
    fields: dimensionFields,
    append: appendDimension,
    remove: removeDimension,
    replace: replaceDimensions,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.dimensions`,
  });

  const {
    fields: measureFields,
    append: appendMeasure,
    remove: removeMeasure,
    replace: replaceMeasures,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.measures`,
  });

  const {
    fields: filterFields,
    append: appendFilter,
    remove: removeFilter,
    replace: replaceFilters,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.filters`,
  });

  const {
    fields: orderFields,
    append: appendOrder,
    remove: removeOrder,
    replace: replaceOrders,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.orders`,
  });

  // Clear fields when topic changes
  useEffect(() => {
    if (
      prevTopicRef.current !== undefined &&
      topicValue !== prevTopicRef.current
    ) {
      replaceDimensions([]);
      replaceMeasures([]);
      replaceFilters([]);
      replaceOrders([]);
    }
    prevTopicRef.current = topicValue;
  }, [
    topicValue,
    replaceDimensions,
    replaceMeasures,
    replaceFilters,
    replaceOrders,
  ]);

  const topicItems = topicFiles.map((t) => ({
    value: t.value,
    label: t.label,
    searchText: t.searchText,
  }));

  const dimensionItems = dimensionOptions.map((d) => ({
    value: d.value,
    label: d.label,
    searchText: d.searchText,
  }));

  const measureItems = measureOptions.map((m) => ({
    value: m.value,
    label: m.label,
    searchText: m.searchText,
  }));

  const allFieldItems = allFieldOptions.map((f) => ({
    value: f.value,
    label: f.label,
    searchText: f.searchText,
  }));

  const renderTopicField = () => {
    if (topicsLoading) {
      return (
        <div className="flex items-center gap-2 h-10 px-3 border rounded-md bg-muted">
          <Loader2 className="w-4 h-4 animate-spin" />
          <span className="text-sm text-muted-foreground">
            Loading topics...
          </span>
        </div>
      );
    }

    if (topicsError) {
      return (
        <Input
          id={`${taskPath}.topic`}
          placeholder="Enter topic path"
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.topic`, {
            required: "Topic is required",
          })}
        />
      );
    }

    return (
      <Controller
        control={control}
        // @ts-expect-error - dynamic field path
        name={`${taskPath}.topic`}
        rules={{ required: "Topic is required" }}
        render={({ field }) => (
          <Combobox
            items={topicItems}
            value={(field.value as string) ?? ""}
            onValueChange={field.onChange}
            placeholder="Select topic..."
            searchPlaceholder="Search topics..."
          />
        )}
      />
    );
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.topic`}>Topic</Label>
        {renderTopicField()}
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
            disabled={!topicValue}
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Dimension
          </Button>
        </div>
        {!topicValue && (
          <p className="text-sm text-muted-foreground">
            Select a topic first to see available dimensions
          </p>
        )}
        {topicValue && fieldsLoading && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="w-4 h-4 animate-spin" />
            Loading dimensions...
          </div>
        )}
        {topicValue && !fieldsLoading && dimensionFields.length > 0 && (
          <div className="space-y-2">
            {dimensionFields.map((field, dimIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.dimensions.${dimIndex}`}
                    render={({ field: controllerField }) => (
                      <Combobox
                        items={dimensionItems}
                        value={controllerField.value as string}
                        onValueChange={controllerField.onChange}
                        placeholder="Select dimension..."
                        searchPlaceholder="Search dimensions..."
                      />
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
        {topicValue && !fieldsLoading && dimensionFields.length === 0 && (
          <p className="text-sm text-muted-foreground">
            Click "Add Dimension" to include dimensions in the query
          </p>
        )}
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Measures</Label>
          <Button
            type="button"
            onClick={() => appendMeasure("" as never)}
            variant="outline"
            size="sm"
            disabled={!topicValue}
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Measure
          </Button>
        </div>
        {!topicValue && (
          <p className="text-sm text-muted-foreground">
            Select a topic first to see available measures
          </p>
        )}
        {topicValue && fieldsLoading && (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="w-4 h-4 animate-spin" />
            Loading measures...
          </div>
        )}
        {topicValue && !fieldsLoading && measureFields.length > 0 && (
          <div className="space-y-2">
            {measureFields.map((field, measureIndex) => (
              <div key={field.id} className="flex gap-2 items-start">
                <div className="flex-1">
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.measures.${measureIndex}`}
                    render={({ field: controllerField }) => (
                      <Combobox
                        items={measureItems}
                        value={controllerField.value as string}
                        onValueChange={controllerField.onChange}
                        placeholder="Select measure..."
                        searchPlaceholder="Search measures..."
                      />
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
        {topicValue && !fieldsLoading && measureFields.length === 0 && (
          <p className="text-sm text-muted-foreground">
            Click "Add Measure" to include measures in the query
          </p>
        )}
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
            disabled={!topicValue}
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
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.filters.${filterIndex}.field`}
                    render={({ field: controllerField }) => (
                      <Combobox
                        items={allFieldItems}
                        value={controllerField.value as string}
                        onValueChange={controllerField.onChange}
                        placeholder="Select field..."
                        searchPlaceholder="Search fields..."
                        disabled={!topicValue || fieldsLoading}
                      />
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
            disabled={!topicValue}
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
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.orders.${orderIndex}.field`}
                    render={({ field: controllerField }) => (
                      <Combobox
                        items={allFieldItems}
                        value={controllerField.value as string}
                        onValueChange={controllerField.onChange}
                        placeholder="Select field..."
                        searchPlaceholder="Search fields..."
                        disabled={!topicValue || fieldsLoading}
                      />
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
