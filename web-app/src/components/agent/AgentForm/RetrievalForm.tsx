import React from "react";
import { useFormContext, useFieldArray } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Button } from "@/components/ui/shadcn/button";
import { Plus, Trash2 } from "lucide-react";
import { AgentFormData } from "./index";
import { CardTitle } from "@/components/ui/shadcn/card";

export const RetrievalForm: React.FC = () => {
  const { control, register } = useFormContext<AgentFormData>();

  const {
    fields: includeFields,
    append: appendInclude,
    remove: removeInclude,
  } = useFieldArray({
    control,
    name: "retrieval.include" as never,
  });

  const {
    fields: excludeFields,
    append: appendExclude,
    remove: removeExclude,
  } = useFieldArray({
    control,
    name: "retrieval.exclude" as never,
  });

  return (
    <div className="space-y-6">
      <CardTitle>Retrieval</CardTitle>

      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h4 className="font-medium">Include Prompts</h4>
            <p className="text-sm text-muted-foreground">
              Prompts that include this agent for retrieval
            </p>
          </div>
          <Button
            type="button"
            onClick={() => appendInclude("")}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-2" />
            Add Include
          </Button>
        </div>

        {includeFields.map((field, index) => (
          <div key={field.id} className="flex items-center gap-2">
            <div className="flex-1">
              <Input
                placeholder="Enter prompt pattern"
                {...register(`retrieval.include.${index}` as never)}
              />
            </div>
            <Button
              type="button"
              onClick={() => removeInclude(index)}
              variant="outline"
              size="sm"
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
        ))}
      </div>

      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h4 className="font-medium">Exclude Prompts</h4>
            <p className="text-sm text-muted-foreground">
              Prompts that exclude this agent from retrieval
            </p>
          </div>
          <Button
            type="button"
            onClick={() => appendExclude("")}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-2" />
            Add Exclude
          </Button>
        </div>

        {excludeFields.length === 0 && (
          <p className="text-center text-muted-foreground py-4">
            No exclude prompts defined.
          </p>
        )}

        {excludeFields.map((field, index) => (
          <div key={field.id} className="flex items-center gap-2">
            <div className="flex-1">
              <Input
                placeholder="Enter prompt pattern"
                {...register(`retrieval.exclude.${index}` as never)}
              />
            </div>
            <Button
              type="button"
              onClick={() => removeExclude(index)}
              variant="outline"
              size="sm"
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
};
