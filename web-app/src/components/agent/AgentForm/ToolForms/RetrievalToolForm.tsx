import React from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { AgentFormData } from "../index";

interface RetrievalToolFormProps {
  index: number;
}

export const RetrievalToolForm: React.FC<RetrievalToolFormProps> = ({
  index,
}) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.src`}>Source Files *</Label>
        <Textarea
          id={`tools.${index}.src`}
          placeholder="Enter file paths (one per line)"
          rows={3}
          {...register(`tools.${index}.src`, {
            required: "Source files are required",
          })}
        />
        <p className="text-sm text-muted-foreground">
          List of file paths to index for retrieval
        </p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`tools.${index}.embed_model`}>Embedding Model</Label>
          <Input
            id={`tools.${index}.embed_model`}
            placeholder="text-embedding-3-small"
            defaultValue="text-embedding-3-small"
            {...register(`tools.${index}.embed_model`)}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`tools.${index}.top_k`}>Top K</Label>
          <Input
            id={`tools.${index}.top_k`}
            type="number"
            min="1"
            defaultValue={4}
            {...register(`tools.${index}.top_k`, {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div className="space-y-2">
          <Label htmlFor={`tools.${index}.n_dims`}>Dimensions</Label>
          <Input
            id={`tools.${index}.n_dims`}
            type="number"
            min="1"
            defaultValue={512}
            placeholder="512"
            {...register(`tools.${index}.n_dims`, {
              valueAsNumber: true,
            })}
          />
        </div>
        <div className="space-y-2">
          <Label htmlFor={`tools.${index}.factor`}>Factor</Label>
          <Input
            id={`tools.${index}.factor`}
            type="number"
            min="1"
            defaultValue={5}
            placeholder="5"
            {...register(`tools.${index}.factor`, {
              valueAsNumber: true,
            })}
          />
        </div>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.api_url`}>API URL</Label>
        <Input
          id={`tools.${index}.api_url`}
          placeholder="https://api.openai.com/v1"
          defaultValue="https://api.openai.com/v1"
          {...register(`tools.${index}.api_url`)}
        />
      </div>

      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.key_var`}>API Key Variable</Label>
        <Input
          id={`tools.${index}.key_var`}
          placeholder="OPENAI_API_KEY"
          defaultValue="OPENAI_API_KEY"
          {...register(`tools.${index}.key_var`)}
        />
        <p className="text-sm text-muted-foreground">
          Environment variable name containing the API key
        </p>
      </div>

      <div className="space-y-2">
        <Label htmlFor={`tools.${index}.db_path`}>Database Path</Label>
        <Input
          id={`tools.${index}.db_path`}
          placeholder=".lancedb"
          defaultValue=".lancedb"
          {...register(`tools.${index}.db_path`)}
        />
        <p className="text-sm text-muted-foreground">
          Path to LanceDB vector database
        </p>
      </div>
    </div>
  );
};
