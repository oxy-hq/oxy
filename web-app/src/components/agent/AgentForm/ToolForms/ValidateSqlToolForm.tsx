import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AgentFormData } from "../index";

interface ValidateSqlToolFormProps {
  index: number;
}

export const ValidateSqlToolForm: React.FC<ValidateSqlToolFormProps> = ({
  index,
}) => {
  const { register } = useFormContext<AgentFormData>();

  return (
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
  );
};
