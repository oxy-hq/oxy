import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Variable } from "..";

interface VariableRowProps {
  variable: Variable;
  onUpdate: (updates: Partial<Variable>) => void;
  onRemove: () => void;
}

const VariableRow = ({ variable, onUpdate, onRemove }: VariableRowProps) => {
  return (
    <div className="flex items-center gap-2">
      <input
        type="text"
        value={variable.key}
        onChange={(e) => onUpdate({ key: e.target.value })}
        placeholder="Variable Name"
        className="text-xs border rounded px-2 py-1 bg-background"
      />
      <span className="text-xs text-muted-foreground">=</span>
      <input
        type="text"
        value={variable.value}
        onChange={(e) => onUpdate({ value: e.target.value })}
        placeholder="Value"
        className="flex-1 text-xs border rounded px-2 py-1 bg-background"
      />
      <Button
        size="sm"
        variant="ghost"
        onClick={onRemove}
        className="h-7 w-7 p-0"
      >
        <X className="w-3 h-3" />
      </Button>
    </div>
  );
};

export default VariableRow;
