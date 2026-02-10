import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Controller, useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Textarea } from "@/components/ui/shadcn/textarea";
import type { AgentFormData } from "./index";
import {
  AgentToolForm,
  CreateDataAppToolForm,
  ExecuteSqlToolForm,
  OmniQueryToolForm,
  RetrievalToolForm,
  SemanticQueryToolForm,
  ValidateSqlToolForm,
  VisualizeToolForm,
  WorkflowToolForm
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
  { value: "semantic_query", label: "Semantic Query" }
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
    formState: { errors }
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
    <div className='rounded-lg border bg-card p-3'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='w-full rounded-lg transition-colors'>
          <div className='flex items-center justify-between transition-colors'>
            {isOpen ? (
              <ChevronDown className='h-5 w-5 text-muted-foreground' />
            ) : (
              <ChevronRight className='h-5 w-5 text-muted-foreground' />
            )}
            <div className='flex flex-1 items-center gap-3'>
              <span className='flex h-8 w-8 items-center justify-center rounded-full bg-primary/10 font-semibold text-primary text-sm'>
                {index + 1}
              </span>
              <div className='flex flex-1 items-center gap-2'>
                <span className='font-medium text-sm'>{toolName || `Tool ${index + 1}`}</span>
                {toolType && (
                  <span className='rounded-md bg-muted px-2 py-1 text-muted-foreground text-xs'>
                    {TOOL_TYPES.find((t) => t.value === toolType)?.label || toolType}
                  </span>
                )}
              </div>
            </div>
            <Button
              type='button'
              onClick={(e) => {
                e.stopPropagation();
                onRemove();
              }}
              variant='ghost'
              size='sm'
            >
              <Trash2 className='h-4 w-4' />
            </Button>
          </div>
        </CollapsibleTrigger>

        <CollapsibleContent className='mt-4 space-y-4'>
          <div className='space-y-4'>
            <div className='space-y-2'>
              <Label htmlFor={`tools.${index}.name`}>Name</Label>
              <Input
                id={`tools.${index}.name`}
                placeholder='Tool name'
                {...register(`tools.${index}.name`)}
              />
              {toolErrors?.name && (
                <p className='text-red-500 text-sm'>{String(toolErrors.name.message || "")}</p>
              )}
            </div>

            <div className='space-y-2'>
              <Label htmlFor={`tools.${index}.type`}>Type *</Label>
              <Controller
                name={`tools.${index}.type`}
                control={control}
                rules={{ required: "Tool type is required" }}
                render={({ field }) => (
                  <Select onValueChange={field.onChange} value={field.value}>
                    <SelectTrigger>
                      <SelectValue placeholder='Select tool type' />
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
                <p className='text-red-500 text-sm'>{String(toolErrors.type.message || "")}</p>
              )}
            </div>

            <div className='space-y-2'>
              <Label htmlFor={`tools.${index}.description`}>Description</Label>
              <Textarea
                id={`tools.${index}.description`}
                placeholder='Describe what this tool does...'
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
    remove: removeTool
  } = useFieldArray({
    control,
    name: "tools"
  });

  return (
    <div className='space-y-4'>
      <div className='flex items-center justify-between'>
        <CardTitle>Tools</CardTitle>
        <Button
          type='button'
          onClick={() =>
            appendTool({
              name: "",
              type: "execute_sql",
              description: ""
            })
          }
          variant='outline'
          size='sm'
        >
          <Plus className='mr-2 h-4 w-4' />
          Add Tool
        </Button>
      </div>

      {toolFields.length === 0 && (
        <p className='py-4 text-center text-muted-foreground'>
          No tools defined. Add tools for the agent to use.
        </p>
      )}

      <div className='space-y-3'>
        {toolFields.map((field, index) => (
          <ToolItemForm key={field.id} index={index} onRemove={() => removeTool(index)} />
        ))}
      </div>
    </div>
  );
};
