import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { WorkflowFormData } from "../..";

interface AgentTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const AgentTaskFields: React.FC<AgentTaskFieldsProps> = ({
  index,
  basePath = "tasks",
}) => {
  const {
    register,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.agent_ref`}>Agent Reference</Label>
        <FilePathAutocompleteInput
          id={`${taskPath}.agent_ref`}
          fileExtension={[".agent.yml", ".agent.yaml"]}
          datalistId={`agent-ref-${basePath}-${index}`}
          placeholder="Enter agent reference"
          // @ts-expect-error - Dynamic path for nested tasks
          {...register(`${taskPath}.agent_ref`, {
            required: "Agent reference is required",
          })}
        />
        {taskErrors?.agent_ref && (
          <p className="text-sm text-red-500">{taskErrors.agent_ref.message}</p>
        )}
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.prompt`}>Prompt</Label>
        <Textarea
          id={`${taskPath}.prompt`}
          placeholder="Enter the prompt for the agent"
          rows={3}
          // @ts-expect-error - Dynamic path for nested tasks
          {...register(`${taskPath}.prompt`, {
            required: "Prompt is required",
          })}
        />
        {taskErrors?.prompt && (
          <p className="text-sm text-red-500">{taskErrors.prompt.message}</p>
        )}
      </div>
      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`${taskPath}.retry`}>Retry Count</Label>
          <Input
            id={`${taskPath}.retry`}
            type="number"
            min="1"
            defaultValue={1}
            // @ts-expect-error - Dynamic path for nested tasks
            {...register(`${taskPath}.retry`, {
              valueAsNumber: true,
            })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`${taskPath}.consistency_run`}>
            Consistency Runs
          </Label>
          <Input
            id={`${taskPath}.consistency_run`}
            type="number"
            min="1"
            defaultValue={1}
            // @ts-expect-error - Dynamic path for nested tasks
            {...register(`${taskPath}.consistency_run`, {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>
    </div>
  );
};
