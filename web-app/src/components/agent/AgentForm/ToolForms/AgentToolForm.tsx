import React from "react";
import { useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { AgentFormData } from "../index";

interface AgentToolFormProps {
  index: number;
}

export const AgentToolForm: React.FC<AgentToolFormProps> = ({ index }) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className="space-y-2">
      <Label htmlFor={`tools.${index}.agent_ref`}>Agent Reference *</Label>
      <FilePathAutocompleteInput
        id={`tools.${index}.agent_ref`}
        fileExtension=".agent.yml"
        datalistId={`tool-agent-ref-${index}`}
        placeholder="Path to agent file"
        {...register(`tools.${index}.agent_ref`, {
          required: "Agent reference is required",
        })}
      />
    </div>
  );
};
