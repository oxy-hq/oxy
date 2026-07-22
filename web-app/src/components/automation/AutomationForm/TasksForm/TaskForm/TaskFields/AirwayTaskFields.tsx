import { Plus, X } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { useFormContext } from "react-hook-form";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Button } from "@/components/ui/shadcn/button";
import { FieldError } from "@/components/ui/shadcn/field";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { AutomationFormData } from "../..";

interface AirwayTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const AirwayTaskFields: React.FC<AirwayTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
  const {
    register,
    watch,
    setValue,
    formState: { errors }
  } = useFormContext<AutomationFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];
  // @ts-expect-error - Dynamic path for nested tasks
  const resources = (watch(`${taskPath}.resources`) as string[] | undefined) || [];
  const [resourceEntries, setResourceEntries] = useState<string[]>(resources);

  const syncResources = (entries: string[]) => {
    setResourceEntries(entries);
    const cleaned = entries.map((e) => e.trim()).filter((e) => e.length > 0);
    setValue(
      // @ts-expect-error - Dynamic path for nested tasks
      `${taskPath}.resources`,
      cleaned.length > 0 ? cleaned : undefined
    );
  };

  const addResource = () => setResourceEntries([...resourceEntries, ""]);
  const removeResource = (indexToRemove: number) =>
    syncResources(resourceEntries.filter((_, i) => i !== indexToRemove));
  const updateResource = (resIndex: number, value: string) => {
    const next = [...resourceEntries];
    next[resIndex] = value;
    syncResources(next);
  };

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.pipeline`}>Pipeline</Label>
        <FilePathAutocompleteInput
          id={`${taskPath}.pipeline`}
          fileExtension='.airway.yml'
          datalistId={`airway-pipelines-${basePath}-${index}`}
          placeholder='Path to the .airway.yml pipeline spec'
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.pipeline`, {
            required: "Pipeline is required"
          })}
        />
        {taskErrors?.pipeline && <FieldError>{taskErrors.pipeline.message}</FieldError>}
      </div>
      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Resources</Label>
          <Button type='button' onClick={addResource} variant='outline' size='sm'>
            <Plus className='mr-1 h-4 w-4' />
            Add Resource
          </Button>
        </div>
        {resourceEntries.length > 0 && (
          <div className='space-y-2'>
            {resourceEntries.map((entry, resIndex) => (
              <div key={resIndex} className='flex items-center gap-2'>
                <div className='flex-1'>
                  <Input
                    placeholder='Resource (table) name'
                    value={entry}
                    onChange={(e) => updateResource(resIndex, e.target.value)}
                  />
                </div>
                <Button
                  className='text-destructive'
                  type='button'
                  onClick={() => removeResource(resIndex)}
                  variant='ghost'
                  size='sm'
                >
                  <X className='h-4 w-4 text-destructive!' />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className='text-muted-foreground text-sm'>
          Restrict the run to specific resources. Leave empty to run the whole pipeline.
        </p>
      </div>
    </div>
  );
};
