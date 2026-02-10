import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { SemanticQueryFilter } from "@/services/api/semantic";
import type { Filter } from "../../../types";
import DateRangeSelector from "./DateRangeSelector";
import DateValueInput from "./DateValueInput";

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
  { label: "Not In Date Range", value: "not_in_date_range" }
];

interface FilterRowProps {
  filter: Filter;
  availableDimensions: { name: string; fullName: string; type?: string }[];
  onUpdate: (updates: Filter) => void;
  onRemove: () => void;
}

const FilterRow = ({ filter, availableDimensions, onUpdate, onRemove }: FilterRowProps) => {
  const selectedDimension = availableDimensions.find((d) => d.fullName === filter.field);
  const isTimeDimension =
    selectedDimension?.type === "date" || selectedDimension?.type === "datetime";
  const dimensionType = selectedDimension?.type || "string";

  const availableOperators = FILTER_OPERATORS.filter((op) => {
    // Only show date range operators for time dimensions
    if (["in_date_range", "not_in_date_range"].includes(op.value)) {
      return isTimeDimension;
    }
    return true;
  });

  const validateValue = (value: string, type: string): boolean => {
    if (!value || value.trim() === "") return true; // Allow empty values

    switch (type) {
      case "number":
        return !Number.isNaN(Number(value));
      case "date":
      case "datetime": {
        // Check if it's a valid date format (ISO or common formats)
        const date = new Date(value);
        return !Number.isNaN(date.getTime());
      }
      case "boolean":
        return ["true", "false", "1", "0"].includes(value.toLowerCase());
      default:
        return true;
    }
  };

  const getInputType = (type: string): string => {
    switch (type) {
      case "number":
        return "number";
      case "date":
        return "date";
      case "datetime":
        return "datetime-local";
      default:
        return "text";
    }
  };

  const handleOperatorChange = (newOp: string) => {
    if (["in", "not_in"].includes(newOp)) {
      onUpdate({
        field: filter.field,
        op: newOp as "in" | "not_in",
        values: "values" in filter ? filter.values : []
      } as SemanticQueryFilter);
    } else if (["in_date_range", "not_in_date_range"].includes(newOp)) {
      const existingFilter = filter as Extract<
        SemanticQueryFilter,
        { op: "in_date_range" | "not_in_date_range" }
      >;
      onUpdate({
        field: filter.field,
        op: newOp as "in_date_range" | "not_in_date_range",
        relative: existingFilter.relative,
        from: existingFilter.from,
        to: existingFilter.to
      } as SemanticQueryFilter);
    } else {
      onUpdate({
        field: filter.field,
        op: newOp as "eq" | "neq" | "gt" | "gte" | "lt" | "lte",
        value: "value" in filter ? filter.value : ""
      } as SemanticQueryFilter);
    }
  };

  return (
    <div className='flex items-center gap-2'>
      <select
        value={filter.field}
        onChange={(e) => {
          const newDimension = availableDimensions.find((d) => d.fullName === e.target.value);
          const newType = newDimension?.type || "string";
          const oldType = dimensionType;

          // Reset value if dimension type changes
          if (newType !== oldType) {
            if (["eq", "neq", "gt", "gte", "lt", "lte"].includes(filter.op)) {
              onUpdate({
                field: e.target.value,
                op: filter.op,
                value: ""
              } as SemanticQueryFilter);
            } else if (["in", "not_in"].includes(filter.op)) {
              onUpdate({
                field: e.target.value,
                op: filter.op,
                values: []
              } as SemanticQueryFilter);
            } else {
              onUpdate({
                ...filter,
                field: e.target.value
              });
            }
          } else {
            onUpdate({
              ...filter,
              field: e.target.value
            });
          }
        }}
        className='rounded border bg-background px-2 py-1 text-xs'
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
        className='rounded border bg-background px-2 py-1 text-xs'
      >
        {availableOperators.map((op) => (
          <option key={op.value} value={op.value}>
            {op.label}
          </option>
        ))}
      </select>
      {["eq", "neq", "gt", "gte", "lt", "lte"].includes(filter.op) &&
        (isTimeDimension ? (
          <div className='flex-1'>
            <DateValueInput
              className='h-6.5 rounded-xs'
              value={"value" in filter && filter.value != null ? String(filter.value) : undefined}
              onChange={(val) =>
                onUpdate({
                  ...filter,
                  value: val ?? ""
                } as SemanticQueryFilter)
              }
              placeholder='Select date...'
            />
          </div>
        ) : (
          <input
            type={getInputType(dimensionType)}
            value={"value" in filter ? String(filter.value ?? "") : ""}
            onChange={(e) => {
              const newValue = e.target.value;

              onUpdate({
                ...filter,
                value: dimensionType === "number" && newValue ? Number(newValue) : newValue
              } as SemanticQueryFilter);
            }}
            placeholder='Value'
            className={`flex-1 rounded border bg-background px-2 py-1 text-xs ${
              "value" in filter &&
              filter.value &&
              !validateValue(String(filter.value), dimensionType)
                ? "border-red-500"
                : ""
            }`}
            title={
              dimensionType === "number"
                ? "Enter a numeric value"
                : dimensionType === "boolean"
                  ? "Enter true or false"
                  : "Enter a value"
            }
          />
        ))}
      {["in", "not_in"].includes(filter.op) && (
        <input
          type='text'
          defaultValue={("values" in filter ? filter.values : []).join(", ")}
          onBlur={(e) => {
            const values = e.target.value
              .split(",")
              .map((v) => v.trim())
              .filter(Boolean);

            onUpdate({
              ...filter,
              values: dimensionType === "number" ? values.map((v) => Number(v)) : values
            } as SemanticQueryFilter);
          }}
          placeholder={
            dimensionType === "number"
              ? "1, 2, 3"
              : dimensionType === "date" || dimensionType === "datetime"
                ? "2024-01-01, 2024-12-31"
                : "Value1, Value2, Value3"
          }
          className='flex-1 rounded border bg-background px-2 py-1 text-xs'
          title={
            dimensionType === "number"
              ? "Enter comma-separated numeric values"
              : dimensionType === "date" || dimensionType === "datetime"
                ? "Enter comma-separated date values"
                : "Enter comma-separated values"
          }
        />
      )}
      {["in_date_range", "not_in_date_range"].includes(filter.op) && (
        <DateRangeSelector
          className='h-6.5 rounded-xs'
          from={"from" in filter ? filter.from : undefined}
          to={"to" in filter ? filter.to : undefined}
          onChange={(updates) =>
            onUpdate({
              ...filter,
              ...updates
            } as SemanticQueryFilter)
          }
        />
      )}
      <Button size='sm' variant='ghost' onClick={onRemove} className='h-7 w-7 p-0'>
        <X className='h-3 w-3' />
      </Button>
    </div>
  );
};

export default FilterRow;
