import { Loader2, Plus, X } from "lucide-react";
import type React from "react";
import { Controller, useFieldArray } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Label } from "@/components/ui/shadcn/label";
import type { MeasuresFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const MeasuresField: React.FC<MeasuresFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  measureItems
}) => {
  const {
    fields: measureFields,
    append: appendMeasure,
    remove: removeMeasure
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.measures`
  });

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between'>
        <Label>Measures</Label>
        <Button
          type='button'
          onClick={() => appendMeasure("" as never)}
          variant='outline'
          size='sm'
          disabled={!topicValue}
        >
          <Plus className='mr-1 h-4 w-4' />
          Add Measure
        </Button>
      </div>
      {!topicValue && (
        <p className='text-muted-foreground text-sm'>
          Select a topic first to see available measures
        </p>
      )}
      {topicValue && fieldsLoading && (
        <div className='flex items-center gap-2 text-muted-foreground text-sm'>
          <Loader2 className='h-4 w-4 animate-spin' />
          Loading measures...
        </div>
      )}
      {topicValue && !fieldsLoading && measureFields.length > 0 && (
        <div className='space-y-2'>
          {measureFields.map((field, measureIndex) => (
            <div key={field.id} className='flex items-start gap-2'>
              <div className='flex-1'>
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
                        placeholder='Select measure...'
                        searchPlaceholder='Search measures...'
                      />
                    );
                  }}
                />
              </div>
              <Button
                type='button'
                onClick={() => removeMeasure(measureIndex)}
                variant='ghost'
                size='sm'
              >
                <X className='h-4 w-4' />
              </Button>
            </div>
          ))}
        </div>
      )}
      {topicValue && !fieldsLoading && measureFields.length === 0 && (
        <p className='text-muted-foreground text-sm'>
          Click "Add Measure" to include measures in the query
        </p>
      )}
    </div>
  );
};
