import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { Field, Order } from "../../../types";

interface SortRowProps {
  order: Order;
  availableFields: Field[];
  onUpdate: (updates: Order) => void;
  onRemove: () => void;
}

const SortRow = ({ order, availableFields, onUpdate, onRemove }: SortRowProps) => {
  return (
    <div className='flex w-full flex-1 items-center gap-2'>
      <select
        value={order.field}
        onChange={(e) =>
          onUpdate({
            ...order,
            field: e.target.value
          })
        }
        className='rounded border bg-background px-2 py-1 text-xs'
      >
        {availableFields.map((field) => (
          <option key={field.fullName} value={field.fullName}>
            {field.name}
          </option>
        ))}
      </select>
      <select
        value={order.direction}
        onChange={(e) =>
          onUpdate({
            ...order,
            direction: e.target.value as "asc" | "desc"
          })
        }
        className='w-34 flex-1 rounded border bg-background px-2 py-1 text-xs'
      >
        <option value='asc'>Ascending</option>
        <option value='desc'>Descending</option>
      </select>
      <Button size='sm' variant='ghost' onClick={onRemove} className='h-7 w-7 p-0'>
        <X className='h-3 w-3' />
      </Button>
    </div>
  );
};

export default SortRow;
