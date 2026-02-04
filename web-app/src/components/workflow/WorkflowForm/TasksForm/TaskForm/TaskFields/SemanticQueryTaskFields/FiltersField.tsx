import { Plus, X } from "lucide-react";
import type React from "react";
import { Controller, useFieldArray } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { FILTER_OPERATORS } from "./constants";
import type { FiltersFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const FiltersField: React.FC<FiltersFieldProps> = ({
  taskPath,
  control,
  register,
  topicValue,
  fieldsLoading,
  allFieldItems
}) => {
  const {
    fields: filterFields,
    append: appendFilter,
    remove: removeFilter
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.filters`
  });

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between'>
        <Label>Filters</Label>
        <Button
          type='button'
          onClick={() => appendFilter({ field: "", op: "eq", value: "" } as never)}
          variant='outline'
          size='sm'
          disabled={!topicValue}
        >
          <Plus className='mr-1 h-4 w-4' />
          Add Filter
        </Button>
      </div>
      {filterFields.length > 0 && (
        <div className='space-y-2'>
          {filterFields.map((field, filterIndex) => (
            <div key={field.id} className='flex items-center gap-2'>
              <div className='flex-1'>
                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.filters.${filterIndex}.field`}
                  render={({ field: controllerField }) => {
                    const value = controllerField.value as string;
                    const items = getItemsWithUnknownValue(allFieldItems, value);
                    return (
                      <Combobox
                        items={items}
                        value={value}
                        onValueChange={controllerField.onChange}
                        placeholder='Select field...'
                        searchPlaceholder='Search fields...'
                        disabled={!topicValue || fieldsLoading}
                      />
                    );
                  }}
                />
              </div>
              <Controller
                control={control}
                // @ts-expect-error - dynamic field path
                name={`${taskPath}.filters.${filterIndex}.op`}
                render={({ field }) => (
                  <Select value={field.value as string} onValueChange={field.onChange}>
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

              <div className='flex-1'>
                <Input
                  placeholder='Value (JSON format)'
                  {...register(
                    // @ts-expect-error - dynamic field path
                    `${taskPath}.filters.${filterIndex}.value`
                  )}
                />
              </div>
              <Button
                type='button'
                onClick={() => removeFilter(filterIndex)}
                variant='ghost'
                size='sm'
              >
                <X className='h-4 w-4' />
              </Button>
            </div>
          ))}
        </div>
      )}
      <p className='text-muted-foreground text-sm'>
        Add filters to narrow down query results. Value should be JSON format (e.g., "value" or
        ["val1", "val2"])
      </p>
    </div>
  );
};
