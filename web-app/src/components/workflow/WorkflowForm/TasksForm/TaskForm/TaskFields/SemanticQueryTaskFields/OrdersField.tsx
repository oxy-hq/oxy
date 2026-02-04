import { Plus, X } from "lucide-react";
import type React from "react";
import { Controller, useFieldArray } from "react-hook-form";
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
import { ORDER_DIRECTIONS } from "./constants";
import type { OrdersFieldProps } from "./types";
import { getItemsWithUnknownValue } from "./utils";

export const OrdersField: React.FC<OrdersFieldProps> = ({
  taskPath,
  control,
  topicValue,
  fieldsLoading,
  allFieldItems
}) => {
  const {
    fields: orderFields,
    append: appendOrder,
    remove: removeOrder
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.orders`
  });

  return (
    <div className='space-y-2'>
      <div className='flex items-center justify-between'>
        <Label>Order By</Label>
        <Button
          type='button'
          onClick={() => appendOrder({ field: "", direction: "asc" } as never)}
          variant='outline'
          size='sm'
          disabled={!topicValue}
        >
          <Plus className='mr-1 h-4 w-4' />
          Add Order
        </Button>
      </div>
      {orderFields.length > 0 && (
        <div className='space-y-2'>
          {orderFields.map((field, orderIndex) => (
            <div key={field.id} className='flex items-center gap-2'>
              <div className='flex-1'>
                <Controller
                  control={control}
                  // @ts-expect-error - dynamic field path
                  name={`${taskPath}.orders.${orderIndex}.field`}
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
                name={`${taskPath}.orders.${orderIndex}.direction`}
                render={({ field }) => (
                  <Select value={field.value as string} onValueChange={field.onChange}>
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
                type='button'
                onClick={() => removeOrder(orderIndex)}
                variant='ghost'
                size='sm'
              >
                <X className='h-4 w-4' />
              </Button>
            </div>
          ))}
        </div>
      )}
      <p className='text-muted-foreground text-sm'>Specify how to sort the query results</p>
    </div>
  );
};
