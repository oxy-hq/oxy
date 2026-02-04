import type React from "react";
import { useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { AppFormData } from "../index";

interface MarkdownDisplayFieldsProps {
  index: number;
}

export const MarkdownDisplayFields: React.FC<MarkdownDisplayFieldsProps> = ({ index }) => {
  const { register } = useFormContext<AppFormData>();

  return (
    <div className='space-y-2'>
      <Label htmlFor={`display.${index}.content`}>Content *</Label>
      <Textarea
        id={`display.${index}.content`}
        placeholder='Enter markdown content'
        rows={8}
        {...register(`display.${index}.content`, {
          required: "Content is required"
        })}
      />
    </div>
  );
};
