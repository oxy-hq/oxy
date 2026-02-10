import { Calendar, Loader2, Plus, X } from "lucide-react";
import type React from "react";
import type { Control } from "react-hook-form";
import { Controller, useFieldArray } from "react-hook-form";
import { DateRangePicker } from "@/components/ui/date-range-picker";
import { Button } from "@/components/ui/shadcn/button";
import { Combobox } from "@/components/ui/shadcn/combobox";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { getItemsWithUnknownValue } from "./utils";

type TimeDimensionsFieldProps = {
  taskPath: string;
  control: Control<any>;
  topicValue?: string;
  fieldsLoading: boolean;
  dimensionItems: Array<{
    value: string;
    label: string;
    type?: "string" | "number" | "date" | "datetime" | "boolean";
  }>;
};

const granularityOptions = [
  { value: "value", label: "value (raw)" },
  { value: "year", label: "year" },
  { value: "quarter", label: "quarter" },
  { value: "month", label: "month" },
  { value: "week", label: "week" },
  { value: "day", label: "day" },
  { value: "hour", label: "hour" },
  { value: "minute", label: "minute" },
  { value: "second", label: "second" }
];

export const TimeDimensionsField: React.FC<TimeDimensionsFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  dimensionItems
}) => {
  const {
    fields: timeDimensionFields,
    append: appendTimeDimension,
    remove: removeTimeDimension
  } = useFieldArray({
    control,
    name: `${taskPath}.time_dimensions`
  });

  // Filter to only date/datetime dimensions
  const timeDimensionItems = dimensionItems
    .filter((d) => d.type === "date" || d.type === "datetime")
    .map((d) => ({
      value: d.value,
      label: d.label,
      searchText: d.label.toLowerCase()
    }));

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between'>
        <Label>Time Dimensions</Label>
        <Button
          type='button'
          onClick={() =>
            appendTimeDimension({
              dimension: "",
              granularity: "value"
            } as never)
          }
          variant='outline'
          size='sm'
          disabled={!topicValue}
        >
          <Plus className='mr-1 h-4 w-4' />
          Add Time Dimension
        </Button>
      </div>
      {!topicValue && (
        <p className='text-muted-foreground text-sm'>
          Select a topic first to see available time dimensions
        </p>
      )}
      {topicValue && fieldsLoading && (
        <div className='flex items-center gap-2 text-muted-foreground text-sm'>
          <Loader2 className='h-4 w-4 animate-spin' />
          Loading time dimensions...
        </div>
      )}
      {topicValue && !fieldsLoading && timeDimensionFields.length > 0 && (
        <div className='space-y-3'>
          {timeDimensionFields.map((field, tdIndex) => (
            <div key={field.id} className='space-y-2 rounded-md border p-3'>
              <div className='flex items-start gap-2'>
                <Calendar className='mt-2.5 h-4 w-4 text-blue-500' />
                <div className='flex-1 space-y-2'>
                  {/* Dimension selector */}
                  <Controller
                    control={control}
                    name={`${taskPath}.time_dimensions.${tdIndex}.dimension`}
                    render={({ field: controllerField }) => {
                      const value = controllerField.value as string;
                      const items = getItemsWithUnknownValue(timeDimensionItems, value);
                      return (
                        <Combobox
                          items={items}
                          value={value}
                          onValueChange={controllerField.onChange}
                          placeholder='Select time dimension...'
                          searchPlaceholder='Search time dimensions...'
                        />
                      );
                    }}
                  />

                  {/* Granularity selector */}
                  <Controller
                    control={control}
                    name={`${taskPath}.time_dimensions.${tdIndex}.granularity`}
                    render={({ field: controllerField }) => (
                      <Select
                        value={controllerField.value || "value"}
                        onValueChange={controllerField.onChange}
                      >
                        <SelectTrigger>
                          <SelectValue placeholder='Select granularity...' />
                        </SelectTrigger>
                        <SelectContent>
                          {granularityOptions.map((option) => (
                            <SelectItem key={option.value} value={option.value}>
                              {option.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  />

                  {/* Date range picker */}
                  <Controller
                    control={control}
                    name={`${taskPath}.time_dimensions.${tdIndex}.dateRange`}
                    render={({ field: controllerField }) => (
                      <DateRangePicker
                        value={controllerField.value}
                        onChange={controllerField.onChange}
                        placeholder='Select date range (optional)'
                        supportsRelative={true}
                      />
                    )}
                  />
                </div>
                <Button
                  type='button'
                  onClick={() => removeTimeDimension(tdIndex)}
                  variant='ghost'
                  size='sm'
                >
                  <X className='h-4 w-4' />
                </Button>
              </div>
            </div>
          ))}
        </div>
      )}
      {topicValue && !fieldsLoading && timeDimensionFields.length === 0 && (
        <p className='text-muted-foreground text-sm'>
          Click "Add Time Dimension" to include time dimensions in the query
        </p>
      )}
    </div>
  );
};
