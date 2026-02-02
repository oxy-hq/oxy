import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { SemanticQueryFilter } from "@/services/api/semantic";
import { Filter } from "../../../types";

const FILTER_OPERATORS = [
  { label: "=", value: "eq" },
  { label: "!=", value: "neq" },
  { label: ">", value: "gt" },
  { label: ">=", value: "gte" },
  { label: "<", value: "lt" },
  { label: "<=", value: "lte" },
  { label: "IN", value: "in" },
  { label: "NOT IN", value: "not_in" },
  { label: "In Date Range", value: "in_date_range" },
  { label: "Not In Date Range", value: "not_in_date_range" },
];

interface FilterRowProps {
  filter: Filter;
  availableDimensions: { name: string; fullName: string }[];
  onUpdate: (updates: Filter) => void;
  onRemove: () => void;
}

const FilterRow = ({
  filter,
  availableDimensions,
  onUpdate,
  onRemove,
}: FilterRowProps) => {
  const handleOperatorChange = (newOp: string) => {
    if (["in", "not_in"].includes(newOp)) {
      onUpdate({
        field: filter.field,
        op: newOp as "in" | "not_in",
        values: "values" in filter ? filter.values : [],
      } as SemanticQueryFilter);
    } else if (["in_date_range", "not_in_date_range"].includes(newOp)) {
      onUpdate({
        field: filter.field,
        op: newOp as "in_date_range" | "not_in_date_range",
        from: "from" in filter ? filter.from : new Date(),
        to: "to" in filter ? filter.to : new Date(),
      } as SemanticQueryFilter);
    } else {
      onUpdate({
        field: filter.field,
        op: newOp as "eq" | "neq" | "gt" | "gte" | "lt" | "lte",
        value: "value" in filter ? filter.value : "",
      } as SemanticQueryFilter);
    }
  };

  return (
    <div className="flex items-center gap-2">
      <select
        value={filter.field}
        onChange={(e) =>
          onUpdate({
            ...filter,
            field: e.target.value,
          })
        }
        className="text-xs border rounded px-2 py-1 bg-background"
      >
        {availableDimensions.map((dim) => (
          <option key={dim.fullName} value={dim.fullName}>
            {dim.name}
          </option>
        ))}
      </select>
      <select
        value={filter.op}
        onChange={(e) => handleOperatorChange(e.target.value)}
        className="text-xs border rounded px-2 py-1 bg-background"
      >
        {FILTER_OPERATORS.map((op) => (
          <option key={op.value} value={op.value}>
            {op.label}
          </option>
        ))}
      </select>
      {["eq", "neq", "gt", "gte", "lt", "lte"].includes(filter.op) && (
        <input
          type="text"
          value={"value" in filter ? String(filter.value ?? "") : ""}
          onChange={(e) =>
            onUpdate({
              ...filter,
              value: e.target.value,
            } as SemanticQueryFilter)
          }
          placeholder="Value"
          className="flex-1 text-xs border rounded px-2 py-1 bg-background"
        />
      )}
      {["in", "not_in"].includes(filter.op) && (
        <input
          type="text"
          defaultValue={("values" in filter ? filter.values : []).join(", ")}
          onBlur={(e) =>
            onUpdate({
              ...filter,
              values: e.target.value
                .split(",")
                .map((v) => v.trim())
                .filter(Boolean),
            } as SemanticQueryFilter)
          }
          placeholder="Value1, Value2, Value3"
          className="flex-1 text-xs border rounded px-2 py-1 bg-background"
        />
      )}
      {["in_date_range", "not_in_date_range"].includes(filter.op) && (
        <>
          <input
            type="datetime-local"
            value={
              "from" in filter
                ? new Date(filter.from).toISOString().slice(0, 16)
                : ""
            }
            onChange={(e) =>
              onUpdate({
                ...filter,
                from: new Date(e.target.value),
              } as SemanticQueryFilter)
            }
            placeholder="From"
            className="flex-1 text-xs border rounded px-2 py-1 bg-background"
          />
          <input
            type="datetime-local"
            value={
              "to" in filter
                ? new Date(filter.to).toISOString().slice(0, 16)
                : ""
            }
            onChange={(e) =>
              onUpdate({
                ...filter,
                to: new Date(e.target.value),
              } as SemanticQueryFilter)
            }
            placeholder="To"
            className="flex-1 text-xs border rounded px-2 py-1 bg-background"
          />
        </>
      )}
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

export default FilterRow;
