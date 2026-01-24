import React from "react";
import { useFieldArray, Controller } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Plus, X, Loader2 } from "lucide-react";
import { DimensionsFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const DimensionsField: React.FC<DimensionsFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  dimensionItems,
}) => {
  const {
    fields: dimensionFields,
    append: appendDimension,
    remove: removeDimension,
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.dimensions`,
  });

  return (
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
                  render={({ field: controllerField }) => {
                    const value = controllerField.value as string;
                    const items = getItemsWithUnknownValue(
                      dimensionItems,
                      value,
                    );
                    return (
                      <Combobox
                        items={items}
                        value={value}
                        onValueChange={controllerField.onChange}
                        placeholder="Select dimension..."
                        searchPlaceholder="Search dimensions..."
                      />
                    );
                  }}
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
  );
};
