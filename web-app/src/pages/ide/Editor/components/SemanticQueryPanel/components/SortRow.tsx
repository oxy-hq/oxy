import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { SemanticQueryOrder } from "@/services/api/semantic";

interface SortRowProps {
  order: SemanticQueryOrder;
  availableFields: { label: string; value: string }[];
  onUpdate: (updates: SemanticQueryOrder) => void;
  onRemove: () => void;
}

const SortRow = ({
  order,
  availableFields,
  onUpdate,
  onRemove,
}: SortRowProps) => {
  return (
    <div className="flex items-center gap-2 w-full flex-1">
      <select
        value={order.field}
        onChange={(e) =>
          onUpdate({
            ...order,
            field: e.target.value,
          })
        }
        className="text-xs border rounded px-2 py-1 bg-background"
      >
        {availableFields.map((field) => (
          <option key={field.value} value={field.value}>
            {field.label}
          </option>
        ))}
      </select>
      <select
        value={order.direction}
        onChange={(e) =>
          onUpdate({
            ...order,
            direction: e.target.value as "asc" | "desc",
          })
        }
        className="text-xs flex-1 w-34 border rounded px-2 py-1 bg-background"
      >
        <option value="asc">Ascending</option>
        <option value="desc">Descending</option>
      </select>
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

export default SortRow;
