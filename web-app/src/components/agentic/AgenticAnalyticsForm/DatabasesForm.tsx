import { Plus, Trash2 } from "lucide-react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Input } from "@/components/ui/shadcn/input";
import type { AgenticFormData } from "./index";

export const DatabasesForm: React.FC = () => {
  const { register } = useFormContext<AgenticFormData>();
  const { fields, append, remove } = useFieldArray<AgenticFormData>({
    name: "databases"
  });

  return (
    <div className='space-y-4'>
      <div className='flex items-center justify-between'>
        <CardTitle>Databases</CardTitle>
        <Button type='button' variant='outline' size='sm' onClick={() => append({ value: "" })}>
          <Plus />
          Add Database
        </Button>
      </div>

      {fields.length === 0 && (
        <p className='py-2 text-center text-muted-foreground text-sm'>
          No databases configured. Uses project default.
        </p>
      )}

      <div className='space-y-2'>
        {fields.map((field, index) => (
          <div key={field.id} className='flex items-center gap-2'>
            <Input
              placeholder='Database name (e.g., training)'
              {...register(`databases.${index}.value`)}
            />
            <Button type='button' variant='ghost' size='sm' onClick={() => remove(index)}>
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
};
