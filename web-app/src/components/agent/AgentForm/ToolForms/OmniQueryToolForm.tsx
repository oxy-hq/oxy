import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { AgentFormData } from "../index";

interface OmniQueryToolFormProps {
  index: number;
}

export const OmniQueryToolForm: React.FC<OmniQueryToolFormProps> = ({
  index,
}) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.integration`}>Integration *</Label>
        <Input
          id={`tools.${index}.integration`}
          placeholder="Integration name"
          {...register(`tools.${index}.integration`, {
            required: "Integration is required",
          })}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.topic`}>Topic *</Label>
        <Input
          id={`tools.${index}.topic`}
          placeholder="Topic name"
          {...register(`tools.${index}.topic`, {
            required: "Topic is required",
          })}
        />
      </div>
    </div>
  );
};
