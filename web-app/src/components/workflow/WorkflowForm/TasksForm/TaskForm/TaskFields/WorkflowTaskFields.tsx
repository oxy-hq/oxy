import { Plus, X } from "lucide-react";
import type React from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { WorkflowFormData } from "../..";

interface WorkflowTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const WorkflowTaskFields: React.FC<WorkflowTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
  const {
    register,
    control,
    formState: { errors }
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;

  const { fields, append, remove } = useFieldArray({
    control,
    // @ts-expect-error - Dynamic field array path
    name: `${taskPath}.variables`
  });

  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.src`}>Source Path</Label>
        <FilePathAutocompleteInput
          id={`${taskPath}.src`}
          fileExtension={[".workflow.yml", ".workflow.yaml", ".automation.yml", ".automation.yaml"]}
          datalistId={`workflow-src-${basePath}-${index}`}
          placeholder='Path to automation file'
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.src`, {
            required: "Source path is required"
          })}
        />
        {taskErrors?.src && <p className='text-red-500 text-sm'>{taskErrors.src.message}</p>}
      </div>

      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Variables</Label>
          <Button
            type='button'
            onClick={() => append({ key: "", value: "" } as never)}
            variant='outline'
            size='sm'
          >
            <Plus className='mr-1 h-4 w-4' />
            Add Variable
          </Button>
        </div>
        {fields.length > 0 && (
          <div className='space-y-2'>
            {fields.map((field, varIndex) => (
              <div key={field.id} className='flex items-center gap-2'>
                <div className='flex-1'>
                  <Input
                    placeholder='Variable name'
                    {...register(
                      // @ts-expect-error - Dynamic field path
                      `${taskPath}.variables.${varIndex}.key`
                    )}
                  />
                </div>
                <div className='flex-1'>
                  <Input
                    placeholder='Variable value (JSON)'
                    {...register(
                      // @ts-expect-error - Dynamic field path
                      `${taskPath}.variables.${varIndex}.value`
                    )}
                  />
                </div>
                <Button type='button' onClick={() => remove(varIndex)} variant='ghost' size='sm'>
                  <X className='h-4 w-4 text-destructive' />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className='text-muted-foreground text-sm'>
          Add variables to pass to the automation. Values should be valid JSON (string, number,
          boolean, object, array, or null).
        </p>
      </div>
    </div>
  );
};
