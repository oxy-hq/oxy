import React from "react";
import { useFieldArray, Controller } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Plus, X, Loader2 } from "lucide-react";
import { MeasuresFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const MeasuresField: React.FC<MeasuresFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  measureItems,
}) => {
  const {
    fields: measureFields,
    append: appendMeasure,
    remove: removeMeasure,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.measures`,
  });

  return (
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
                  render={({ field: controllerField }) => {
                    const value = controllerField.value as string;
                    const items = getItemsWithUnknownValue(measureItems, value);
                    return (
                      <Combobox
                        items={items}
                        value={value}
                        onValueChange={controllerField.onChange}
                        placeholder="Select measure..."
                        searchPlaceholder="Search measures..."
                      />
                    );
                  }}
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
  );
};
