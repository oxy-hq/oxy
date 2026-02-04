import React from "react";
import { useFormContext } from "react-hook-form";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { AgentFormData } from "../index";

interface WorkflowToolFormProps {
  index: number;
}

export const WorkflowToolForm: React.FC<WorkflowToolFormProps> = ({ index }) => {
  const { register, setValue, watch } = useFormContext<AgentFormData>();

  const variablesValue = watch(`tools.${index}.variables`) as unknown | undefined;
  const [variablesError, setVariablesError] = React.useState<string | null>(null);

  const handleVariablesChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const value = e.target.value;
    if (!value.trim()) {
      setVariablesError(null);
      setValue(`tools.${index}.variables`, undefined);
      return;
    }

    try {
      const parsed = JSON.parse(value);
      setVariablesError(null);
      setValue(`tools.${index}.variables`, parsed);
    } catch {
      setValue(`tools.${index}.variables`, value);
      setVariablesError("Invalid JSON format");
    }
  };

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`tools.${index}.workflow_ref`}>Workflow Reference *</Label>
        <FilePathAutocompleteInput
          id={`tools.${index}.workflow_ref`}
          fileExtension='.workflow.yml'
          datalistId={`tool-workflow-ref-${index}`}
          placeholder='Path to workflow file'
          {...register(`tools.${index}.workflow_ref`, {
            required: "Workflow reference is required"
          })}
        />
      </div>
      <div className='space-y-2'>
        <Label htmlFor={`tools.${index}.output_task_ref`}>Output Task Reference</Label>
        <Input
          id={`tools.${index}.output_task_ref`}
          placeholder='Optional task reference for output'
          {...register(`tools.${index}.output_task_ref`)}
        />
      </div>
      <div className='space-y-2'>
        <Label htmlFor={`tools.${index}.variables`}>Variables (JSON Schema)</Label>
        <Textarea
          id={`tools.${index}.variables`}
          placeholder='{"param_name": {"type": "string", "description": "Parameter description"}}'
          rows={6}
          className={variablesError ? "border-red-500" : ""}
          defaultValue={variablesValue ? JSON.stringify(variablesValue, null, 2) : ""}
          onChange={handleVariablesChange}
        />
        {variablesError && <p className='text-red-500 text-sm'>{variablesError}</p>}
        <p className='text-muted-foreground text-sm'>
          Define workflow input parameters with JSON Schema (optional)
        </p>
      </div>
    </div>
  );
};
