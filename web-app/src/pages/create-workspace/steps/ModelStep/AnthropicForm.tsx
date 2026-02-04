import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import type { ModelsFormData } from "./index";

interface AnthropicFormProps {
  index: number;
}

export default function AnthropicForm({ index }: AnthropicFormProps) {
  const {
    register,
    formState: { errors }
  } = useFormContext<ModelsFormData>();
  const fieldErrors = errors?.models?.[index]?.config;

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`models.${index}.config.model_ref`}>Model Reference</Label>
        <Input
          id={`models.${index}.config.model_ref`}
          placeholder='claude-3-sonnet-20240229'
          {...register(`models.${index}.config.model_ref`, {
            required: "Model reference is required"
          })}
        />
        {fieldErrors?.model_ref && (
          <p className='mt-1 text-destructive text-xs'>
            {fieldErrors.model_ref.message?.toString()}
          </p>
        )}
        <p className='text-muted-foreground text-xs'>
          The model identifier (e.g., claude-3-opus, claude-3-sonnet)
        </p>
      </div>

      <div className='space-y-2'>
        <Label htmlFor={`models.${index}.config.api_key`}>API Key</Label>
        <Input
          id={`models.${index}.config.api_key`}
          placeholder='ANTHROPIC_API_KEY'
          {...register(`models.${index}.config.api_key`, {
            required: "API key is required"
          })}
        />
        {fieldErrors?.api_key && (
          <p className='mt-1 text-destructive text-xs'>{fieldErrors.api_key.message?.toString()}</p>
        )}
      </div>

      <div className='space-y-2'>
        <Label htmlFor={`models.${index}.config.api_url`}>API URL (Optional)</Label>
        <Input
          id={`models.${index}.config.api_url`}
          placeholder='https://api.anthropic.com/v1'
          {...register(`models.${index}.config.api_url`)}
        />
      </div>
    </div>
  );
}
