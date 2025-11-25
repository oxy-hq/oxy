import React, { useState } from "react";
import { useFormContext, useFieldArray, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
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
import {
  ExecuteSqlToolForm,
  ValidateSqlToolForm,
  WorkflowToolForm,
  AgentToolForm,
  OmniQueryToolForm,
  SemanticQueryToolForm,
  RetrievalToolForm,
  VisualizeToolForm,
  CreateDataAppToolForm,
} from "./ToolForms";

const TOOL_TYPES = [
  { value: "execute_sql", label: "Execute SQL" },
  { value: "validate_sql", label: "Validate SQL" },
  { value: "retrieval", label: "Retrieval" },
  { value: "visualize", label: "Visualize" },
  { value: "workflow", label: "Workflow" },
  { value: "agent", label: "Agent" },
  { value: "create_data_app", label: "Create Data App" },
  { value: "omni_query", label: "Omni Query" },
  { value: "semantic_query", label: "Semantic Query" },
];

interface ToolItemFormProps {
  index: number;
  onRemove: () => void;
}

const ToolItemForm: React.FC<ToolItemFormProps> = ({ index, onRemove }) => {
  const [isOpen, setIsOpen] = useState(false);
  const {
    register,
    control,
    watch,
    formState: { errors },
  } = useFormContext<AgentFormData>();

  const toolType = watch(`tools.${index}.type`);
  const toolName = watch(`tools.${index}.name`);
  const toolErrors = errors.tools?.[index];

  const renderToolSpecificFields = () => {
    switch (toolType) {
      case "execute_sql":
        return <ExecuteSqlToolForm index={index} />;
      case "validate_sql":
        return <ValidateSqlToolForm index={index} />;
      case "workflow":
        return <WorkflowToolForm index={index} />;
      case "agent":
        return <AgentToolForm index={index} />;
      case "omni_query":
        return <OmniQueryToolForm index={index} />;
      case "semantic_query":
        return <SemanticQueryToolForm index={index} />;
      case "retrieval":
        return <RetrievalToolForm index={index} />;
      case "visualize":
        return <VisualizeToolForm index={index} />;
      case "create_data_app":
        return <CreateDataAppToolForm index={index} />;
      default:
        return null;
    }
  };

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
                  {toolName || `Tool ${index + 1}`}
                </span>
                {toolType && (
                  <span className="text-xs px-2 py-1 rounded-md bg-muted text-muted-foreground">
                    {TOOL_TYPES.find((t) => t.value === toolType)?.label ||
                      toolType}
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
              <Label htmlFor={`tools.${index}.name`}>Name *</Label>
              <Input
                id={`tools.${index}.name`}
                placeholder="Tool name"
                {...register(`tools.${index}.name`, {
                  required: "Tool name is required",
                })}
              />
              {toolErrors?.name && (
                <p className="text-sm text-red-500">
                  {String(toolErrors.name.message || "")}
                </p>
              )}
            </div>

            <div className="space-y-2">
              <Label htmlFor={`tools.${index}.type`}>Type *</Label>
              <Controller
                name={`tools.${index}.type`}
                control={control}
                rules={{ required: "Tool type is required" }}
                render={({ field }) => (
                  <Select onValueChange={field.onChange} value={field.value}>
                    <SelectTrigger>
                      <SelectValue placeholder="Select tool type" />
                    </SelectTrigger>
                    <SelectContent>
                      {TOOL_TYPES.map((type) => (
                        <SelectItem key={type.value} value={type.value}>
                          {type.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
              {toolErrors?.type && (
                <p className="text-sm text-red-500">
                  {String(toolErrors.type.message || "")}
                </p>
              )}
            </div>

            <div className="space-y-2">
              <Label htmlFor={`tools.${index}.description`}>Description</Label>
              <Textarea
                id={`tools.${index}.description`}
                placeholder="Describe what this tool does..."
                rows={3}
                {...register(`tools.${index}.description`)}
              />
            </div>

            {renderToolSpecificFields()}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

export const ToolsForm: React.FC = () => {
  const { control } = useFormContext<AgentFormData>();

  const {
    fields: toolFields,
    append: appendTool,
    remove: removeTool,
  } = useFieldArray({
    control,
    name: "tools",
  });

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <CardTitle>Tools</CardTitle>
        <Button
          type="button"
          onClick={() =>
            appendTool({
              name: "",
              type: "execute_sql",
              description: "",
            })
          }
          variant="outline"
          size="sm"
        >
          <Plus className="w-4 h-4 mr-2" />
          Add Tool
        </Button>
      </div>

      {toolFields.length === 0 && (
        <p className="text-center text-muted-foreground py-4">
          No tools defined. Add tools for the agent to use.
        </p>
      )}

      <div className="space-y-3">
        {toolFields.map((field, index) => (
          <ToolItemForm
            key={field.id}
            index={index}
            onRemove={() => removeTool(index)}
          />
        ))}
      </div>
    </div>
  );
};
