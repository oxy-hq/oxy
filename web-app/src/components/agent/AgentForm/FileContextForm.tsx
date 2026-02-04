import { Plus, Trash2 } from "lucide-react";
import type React from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { AgentFormData } from "./index";

interface FileContextFormProps {
  index: number;
}

export const FileContextForm: React.FC<FileContextFormProps> = ({ index }) => {
  const { register, control } = useFormContext<AgentFormData>();

  const {
    fields: filePathFields,
    append: appendFilePath,
    remove: removeFilePath
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path for primitive array
    name: `context.${index}.src`
  });

  return (
    <div className='space-y-2'>
      <Label htmlFor={`context.${index}.src`}>File Paths</Label>

      {/* Display existing file paths */}
      {filePathFields.length > 0 && (
        <div className='space-y-2'>
          {filePathFields.map((field, pathIndex) => (
            <div key={field.id} className='flex items-center justify-between gap-2'>
              <Input
                placeholder='File path'
                {...register(`context.${index}.src.${pathIndex}`)}
                className='flex-1'
              />
              <Button
                type='button'
                variant='ghost'
                size='sm'
                onClick={() => removeFilePath(pathIndex)}
                className='h-9 w-9 p-0'
              >
                <Trash2 className='h-4 w-4' />
              </Button>
            </div>
          ))}
        </div>
      )}

      {/* Add new file path button */}
      <Button
        type='button'
        onClick={() => appendFilePath("" as never)}
        variant='outline'
        size='sm'
        className='w-full'
      >
        <Plus className='mr-2 h-4 w-4' />
        Add File Path
      </Button>
      <p className='text-muted-foreground text-sm'>Add file paths for this context source.</p>
    </div>
  );
};
