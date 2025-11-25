import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AgentFormData } from "../index";

interface ExecuteSqlToolFormProps {
  index: number;
}

export const ExecuteSqlToolForm: React.FC<ExecuteSqlToolFormProps> = ({
  index,
}) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.database`}>Database *</Label>
        <Input
          id={`tools.${index}.database`}
          placeholder="Database name"
          {...register(`tools.${index}.database`, {
            required: "Database is required",
          })}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.dry_run_limit`}>Dry Run Limit</Label>
        <Input
          id={`tools.${index}.dry_run_limit`}
          type="number"
          min="0"
          placeholder="Optional limit for dry runs"
          {...register(`tools.${index}.dry_run_limit`, {
            valueAsNumber: true,
          })}
        />
      </div>
    </div>
  );
};
