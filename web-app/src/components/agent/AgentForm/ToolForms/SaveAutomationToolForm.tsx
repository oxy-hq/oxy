import { Plus, Trash2 } from "lucide-react";
import type React from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import type { AgentFormData } from "../index";

interface SaveAutomationToolFormProps {
  index: number;
}

export const SaveAutomationToolForm: React.FC<SaveAutomationToolFormProps> = ({ index }) => {
  const { control, register } = useFormContext<AgentFormData>();

  const {
    fields: includeFields,
    append: appendInclude,
    remove: removeInclude
  } = useFieldArray({
    control,
    name: `tools.${index}.retrieval.include` as never
  });

  const {
    fields: excludeFields,
    append: appendExclude,
    remove: removeExclude
  } = useFieldArray({
    control,
    name: `tools.${index}.retrieval.exclude` as never
  });

  return (
    <div className='space-y-6'>
      <div className='space-y-4'>
        <div className='flex items-center justify-between'>
          <div>
            <h4 className='font-medium'>Include Prompts</h4>
            <p className='text-muted-foreground text-sm'>
              Prompts that include this tool for retrieval
            </p>
          </div>
          <Button type='button' onClick={() => appendInclude("")} variant='outline' size='sm'>
            <Plus className='mr-2 h-4 w-4' />
            Add Include
          </Button>
        </div>

        {includeFields.map((field, i) => (
          <div key={field.id} className='flex items-center gap-2'>
            <div className='flex-1'>
              <Input
                placeholder='Enter prompt pattern'
                {...register(`tools.${index}.retrieval.include.${i}` as never)}
              />
            </div>
            <Button type='button' onClick={() => removeInclude(i)} variant='outline' size='sm'>
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        ))}
      </div>

      <div className='space-y-4'>
        <div className='flex items-center justify-between'>
          <div>
            <h4 className='font-medium'>Exclude Prompts</h4>
            <p className='text-muted-foreground text-sm'>
              Prompts that exclude this tool from retrieval
            </p>
          </div>
          <Button type='button' onClick={() => appendExclude("")} variant='outline' size='sm'>
            <Plus className='mr-2 h-4 w-4' />
            Add Exclude
          </Button>
        </div>

        {excludeFields.map((field, i) => (
          <div key={field.id} className='flex items-center gap-2'>
            <div className='flex-1'>
              <Input
                placeholder='Enter prompt pattern'
                {...register(`tools.${index}.retrieval.exclude.${i}` as never)}
              />
            </div>
            <Button type='button' onClick={() => removeExclude(i)} variant='outline' size='sm'>
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
};
