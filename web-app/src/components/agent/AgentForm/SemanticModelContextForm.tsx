import React from "react";
import { useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { AgentFormData } from "./index";

interface SemanticModelContextFormProps {
  index: number;
}

export const SemanticModelContextForm: React.FC<
  SemanticModelContextFormProps
> = ({ index }) => {
  const { register } = useFormContext<AgentFormData>();

  return (
    <div className="space-y-2">
      <Label htmlFor={`context.${index}.src`}>Semantic Model Path</Label>
      <FilePathAutocompleteInput
        id={`context.${index}.src`}
        fileExtension=".yml"
        datalistId={`context-src-semantic-${index}`}
        placeholder="Enter semantic model path"
        {...register(`context.${index}.src`)}
      />
      <p className="text-sm text-muted-foreground">
        Path to semantic model configuration file.
      </p>
    </div>
  );
};
