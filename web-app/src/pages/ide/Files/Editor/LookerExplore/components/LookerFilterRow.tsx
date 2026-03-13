import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import type { LookerFilter } from "../contexts/LookerExplorerContext";

interface LookerFilterRowProps {
  filter: LookerFilter;
  availableFields: string[];
  onUpdate: (updates: LookerFilter) => void;
  onRemove: () => void;
}

const LookerFilterRow = ({ filter, availableFields, onUpdate, onRemove }: LookerFilterRowProps) => (
  <div className='flex items-center gap-2'>
    <Select value={filter.field} onValueChange={(value) => onUpdate({ ...filter, field: value })}>
      <SelectTrigger size='sm' className='w-40'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {availableFields.map((f) => (
          <SelectItem key={f} value={f}>
            {f}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
    <Input
      value={filter.value}
      onChange={(e) => onUpdate({ ...filter, value: e.target.value })}
      placeholder='Filter expression...'
      className='h-7 flex-1 text-xs'
    />
    <Button size='sm' variant='ghost' onClick={onRemove} className='h-7 w-7 p-0'>
      <X className='h-3 w-3' />
    </Button>
  </div>
);

export default LookerFilterRow;
