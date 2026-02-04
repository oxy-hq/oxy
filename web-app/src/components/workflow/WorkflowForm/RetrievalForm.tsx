import { Plus, Trash2 } from "lucide-react";
import type React from "react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import type { WorkflowFormData } from "./index";

export const RetrievalForm: React.FC = () => {
  const { control, register } = useFormContext<WorkflowFormData>();

  const {
    fields: includeFields,
    append: appendInclude,
    remove: removeInclude
  } = useFieldArray({
    control,
    name: "retrieval.include" as never
  });

  const {
    fields: excludeFields,
    append: appendExclude,
    remove: removeExclude
  } = useFieldArray({
    control,
    name: "retrieval.exclude" as never
  });

  return (
    <div className='space-y-6'>
      <CardTitle>Retrieval</CardTitle>

      <div className='space-y-4'>
        <div className='flex items-center justify-between'>
          <div>
            <h4 className='font-medium'>Include Prompts</h4>
            <p className='text-muted-foreground text-sm'>
              Prompts that include this document/route for retrieval
            </p>
          </div>
          <Button type='button' onClick={() => appendInclude("")} variant='outline' size='sm'>
            <Plus className='mr-2 h-4 w-4' />
            Add Include
          </Button>
        </div>

        {includeFields.map((field, index) => (
          <div key={field.id} className='flex items-center gap-2'>
            <div className='flex-1'>
              <Input
                placeholder='Enter prompt pattern'
                {...register(`retrieval.include.${index}` as never)}
              />
            </div>
            <Button type='button' onClick={() => removeInclude(index)} variant='outline' size='sm'>
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
              Prompts that exclude this document/route from retrieval
            </p>
          </div>
          <Button type='button' onClick={() => appendExclude("")} variant='outline' size='sm'>
            <Plus className='mr-2 h-4 w-4' />
            Add Exclude
          </Button>
        </div>

        {excludeFields.length === 0 && (
          <p className='py-4 text-center text-muted-foreground'>No exclude prompts defined.</p>
        )}

        {excludeFields.map((field, index) => (
          <div key={field.id} className='flex items-center gap-2'>
            <div className='flex-1'>
              <Input
                placeholder='Enter prompt pattern'
                {...register(`retrieval.exclude.${index}` as never)}
              />
            </div>
            <Button type='button' onClick={() => removeExclude(index)} variant='outline' size='sm'>
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
};
