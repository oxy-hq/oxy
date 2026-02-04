import { Loader2, Plus, X } from "lucide-react";
import type React from "react";
import { Controller, useFieldArray } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Label } from "@/components/ui/shadcn/label";
import type { DimensionsFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const DimensionsField: React.FC<DimensionsFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  dimensionItems
}) => {
  const {
    fields: dimensionFields,
    append: appendDimension,
    remove: removeDimension
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.dimensions`
  });

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between'>
        <Label>Dimensions</Label>
        <Button
          type='button'
          onClick={() => appendDimension("" as never)}
          variant='outline'
          size='sm'
          disabled={!topicValue}
        >
          <Plus className='mr-1 h-4 w-4' />
          Add Dimension
        </Button>
      </div>
      {!topicValue && (
        <p className='text-muted-foreground text-sm'>
          Select a topic first to see available dimensions
        </p>
      )}
      {topicValue && fieldsLoading && (
        <div className='flex items-center gap-2 text-muted-foreground text-sm'>
          <Loader2 className='h-4 w-4 animate-spin' />
          Loading dimensions...
        </div>
      )}
      {topicValue && !fieldsLoading && dimensionFields.length > 0 && (
        <div className='space-y-2'>
          {dimensionFields.map((field, dimIndex) => (
            <div key={field.id} className='flex items-start gap-2'>
              <div className='flex-1'>
                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.dimensions.${dimIndex}`}
                  render={({ field: controllerField }) => {
                    const value = controllerField.value as string;
                    const items = getItemsWithUnknownValue(dimensionItems, value);
                    return (
                      <Combobox
                        items={items}
                        value={value}
                        onValueChange={controllerField.onChange}
                        placeholder='Select dimension...'
                        searchPlaceholder='Search dimensions...'
                      />
                    );
                  }}
                />
              </div>
              <Button
                type='button'
                onClick={() => removeDimension(dimIndex)}
                variant='ghost'
                size='sm'
              >
                <X className='h-4 w-4' />
              </Button>
            </div>
          ))}
        </div>
      )}
      {topicValue && !fieldsLoading && dimensionFields.length === 0 && (
        <p className='text-muted-foreground text-sm'>
          Click "Add Dimension" to include dimensions in the query
        </p>
      )}
    </div>
  );
};
