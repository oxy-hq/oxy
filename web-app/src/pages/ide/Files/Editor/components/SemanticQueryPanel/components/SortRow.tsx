import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { Field, Order } from "../../../types";

interface SortRowProps {
  order: Order;
  availableFields: Field[];
  onUpdate: (updates: Order) => void;
  onRemove: () => void;
}

const SortRow = ({ order, availableFields, onUpdate, onRemove }: SortRowProps) => {
  return (
    <div className='flex w-full items-center gap-2'>
      <Select value={order.field} onValueChange={(val) => onUpdate({ ...order, field: val })}>
        <SelectTrigger className='min-w-0 flex-1'>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {availableFields.map((field) => (
            <SelectItem className='cursor-pointer' key={field.fullName} value={field.fullName}>
              {field.name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select
        value={order.direction}
        onValueChange={(val) => onUpdate({ ...order, direction: val as "asc" | "desc" })}
      >
        <SelectTrigger>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem className='cursor-pointer' value='asc'>
            Ascending
          </SelectItem>
          <SelectItem className='cursor-pointer' value='desc'>
            Descending
          </SelectItem>
        </SelectContent>
      </Select>

      <Button size='sm' variant='ghost' onClick={onRemove}>
        <X />
      </Button>
    </div>
  );
};

export default SortRow;
