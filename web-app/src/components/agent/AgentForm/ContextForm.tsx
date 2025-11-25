import { useState } from "react";
import { useFormContext, useFieldArray, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Button } from "@/components/ui/shadcn/button";
import { Plus, Trash2, ChevronDown, ChevronRight } from "lucide-react";
import { CardTitle } from "@/components/ui/shadcn/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/shadcn/collapsible";
import { AgentFormData } from "./index";
import { FileContextForm } from "./FileContextForm";
import { SemanticModelContextForm } from "./SemanticModelContextForm";

const CONTEXT_TYPES = [
  { value: "file", label: "File" },
  { value: "semantic_model", label: "Semantic Model" },
];

interface ContextItemFormProps {
  index: number;
  onRemove: () => void;
}

const ContextItemForm: React.FC<ContextItemFormProps> = ({
  index,
  onRemove,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const {
    register,
    control,
    watch,
    formState: { errors },
  } = useFormContext<AgentFormData>();

  const contextType = watch(`context.${index}.type`);
  const contextName = watch(`context.${index}.name`);
  const contextErrors = errors.context?.[index];

  return (
    <div className="rounded-lg border bg-card p-3">
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className="rounded-lg transition-colors w-full">
          <div className="flex items-center justify-between transition-colors">
            {isOpen ? (
              <ChevronDown className="h-5 w-5 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-5 w-5 text-muted-foreground" />
            )}
            <div className="flex items-center gap-3 flex-1">
              <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary font-semibold text-sm">
                {index + 1}
              </span>
              <div className="flex items-center gap-2 flex-1">
                <span className="font-medium text-sm">
                  {contextName || `Context ${index + 1}`}
                </span>
                {contextType && (
                  <span className="text-xs px-2 py-1 rounded-md bg-muted text-muted-foreground">
                    {CONTEXT_TYPES.find((t) => t.value === contextType)
                      ?.label || contextType}
                  </span>
                )}
              </div>
            </div>
            <Button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onRemove();
              }}
              variant="ghost"
              size="sm"
            >
              <Trash2 className="w-4 h-4" />
            </Button>
          </div>
        </CollapsibleTrigger>

        <CollapsibleContent className="space-y-4 mt-4">
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor={`context.${index}.name`}>Name *</Label>
              <Input
                id={`context.${index}.name`}
                placeholder="Context name"
                {...register(`context.${index}.name`, {
                  required: "Context name is required",
                })}
              />
              {contextErrors?.name && (
                <p className="text-sm text-red-500">
                  {String(contextErrors.name.message || "")}
                </p>
              )}
            </div>

            <div className="space-y-2">
              <Label htmlFor={`context.${index}.type`}>Type *</Label>
              <Controller
                name={`context.${index}.type`}
                control={control}
                rules={{ required: "Context type is required" }}
                render={({ field }) => (
                  <Select onValueChange={field.onChange} value={field.value}>
                    <SelectTrigger>
                      <SelectValue placeholder="Select context type" />
                    </SelectTrigger>
                    <SelectContent>
                      {CONTEXT_TYPES.map((type) => (
                        <SelectItem key={type.value} value={type.value}>
                          {type.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
              {contextErrors?.type && (
                <p className="text-sm text-red-500">
                  {String(contextErrors.type.message || "")}
                </p>
              )}
            </div>

            {contextType === "file" && <FileContextForm index={index} />}
            {contextType === "semantic_model" && (
              <SemanticModelContextForm index={index} />
            )}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

export const ContextForm: React.FC = () => {
  const { control } = useFormContext<AgentFormData>();

  const {
    fields: contextFields,
    append: appendContext,
    remove: removeContext,
  } = useFieldArray({
    control,
    name: "context",
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <CardTitle>Context</CardTitle>
        <Button
          type="button"
          onClick={() =>
            appendContext({
              name: "",
              type: "file",
              src: "",
            })
          }
          variant="outline"
          size="sm"
        >
          <Plus className="w-4 h-4 mr-2" />
          Add Context
        </Button>
      </div>

      {contextFields.length === 0 && (
        <p className="text-center text-muted-foreground py-4">
          No context defined. Add context sources for the agent.
        </p>
      )}

      <div className="space-y-3">
        {contextFields.map((field, index) => (
          <ContextItemForm
            key={field.id}
            index={index}
            onRemove={() => removeContext(index)}
          />
        ))}
      </div>
    </div>
  );
};
