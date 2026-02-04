import type React from "react";
import { useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { WorkflowFormData } from "../..";

interface FormatterTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const FormatterTaskFields: React.FC<FormatterTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
  const {
    register,
    formState: { errors }
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.template`}>Template</Label>
        <Textarea
          id={`${taskPath}.template`}
          placeholder='Enter template content'
          rows={4}
          // @ts-expect-error - Dynamic path for nested tasks
          {...register(`${taskPath}.template`, {
            required: "Template is required"
          })}
        />
        {taskErrors?.template && (
          <p className='text-red-500 text-sm'>{taskErrors.template.message}</p>
        )}
      </div>
    </div>
  );
};
