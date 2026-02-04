import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { Variable } from "..";

interface VariableRowProps {
  variable: Variable;
  onUpdate: (updates: Partial<Variable>) => void;
  onRemove: () => void;
}

const VariableRow = ({ variable, onUpdate, onRemove }: VariableRowProps) => {
  return (
    <div className='flex items-center gap-2'>
      <input
        type='text'
        value={variable.key}
        onChange={(e) => onUpdate({ key: e.target.value })}
        placeholder='Variable Name'
        className='w-32 rounded border bg-background px-2 py-1 text-xs'
      />
      <input
        type='text'
        value={variable.value}
        onChange={(e) => onUpdate({ value: e.target.value })}
        placeholder='Value'
        className='flex-1 rounded border bg-background px-2 py-1 text-xs'
      />
      <Button size='sm' variant='ghost' onClick={onRemove} className='h-7 w-7 p-0'>
        <X className='h-3 w-3' />
      </Button>
    </div>
  );
};

export default VariableRow;
