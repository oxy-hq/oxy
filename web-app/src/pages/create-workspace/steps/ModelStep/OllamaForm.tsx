import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { useFormContext } from "react-hook-form";
import { ModelsFormData } from "./index";

interface OllamaFormProps {
  index: number;
}

export default function OllamaForm({ index }: OllamaFormProps) {
  const {
    register,
    formState: { errors },
  } = useFormContext<ModelsFormData>();
  const fieldErrors = errors?.models?.[index]?.config;

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`models.${index}.config.model_ref`}>
          Model Reference
        </Label>
        <Input
          id={`models.${index}.config.model_ref`}
          placeholder="llama3"
          {...register(`models.${index}.config.model_ref`, {
            required: "Model reference is required",
          })}
        />
        {fieldErrors?.model_ref && (
          <p className="text-xs text-destructive mt-1">
            {fieldErrors.model_ref.message?.toString()}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          The model identifier (e.g., llama3, mistral, etc.)
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`models.${index}.config.api_key`}>API Key</Label>
        <Input
          id={`models.${index}.config.api_key`}
          placeholder="OLLAMA_AI_API_KEY"
          {...register(`models.${index}.config.api_key`, {
            required: "API key is required",
          })}
        />
        {fieldErrors?.api_key && (
          <p className="text-xs text-destructive mt-1">
            {fieldErrors.api_key.message?.toString()}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <Label htmlFor={`models.${index}.config.api_url`}>API URL</Label>
        <Input
          id={`models.${index}.config.api_url`}
          placeholder="http://localhost:11434"
          {...register(`models.${index}.config.api_url`, {
            required: "API URL is required",
          })}
        />
        {fieldErrors?.api_url && (
          <p className="text-xs text-destructive mt-1">
            {fieldErrors.api_url.message?.toString()}
          </p>
        )}
        <p className="text-xs text-muted-foreground">
          The URL of your Ollama instance
        </p>
      </div>
    </div>
  );
}
