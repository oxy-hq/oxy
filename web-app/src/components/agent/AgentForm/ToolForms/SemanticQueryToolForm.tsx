import type React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { AgentFormData } from "../index";

interface SemanticQueryToolFormProps {
  index: number;
}

export const SemanticQueryToolForm: React.FC<SemanticQueryToolFormProps> = ({ index }) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`tools.${index}.topic`}>Topic</Label>
        <Input
          id={`tools.${index}.topic`}
          placeholder='Optional topic'
          {...register(`tools.${index}.topic`)}
        />
      </div>
      <div className='space-y-2'>
        <Label htmlFor={`tools.${index}.dry_run_limit`}>Dry Run Limit</Label>
        <Input
          id={`tools.${index}.dry_run_limit`}
          type='number'
          min='0'
          placeholder='Optional limit for dry runs'
          {...register(`tools.${index}.dry_run_limit`, {
            valueAsNumber: true
          })}
        />
      </div>
    </div>
  );
};
