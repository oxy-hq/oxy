import { PlusIcon, X as XIcon } from "lucide-react";
import { useFieldArray, useFormContext } from "react-hook-form";
import { v4 as uuidv4 } from "uuid";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
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
import type { ModelsFormData } from "../ModelStep";
import type { WarehousesFormData } from "../WarehouseStep";
import type { AgentConfig, ExecuteSQLToolConfig, ToolConfig } from "./index";

type ToolType = "execute_sql" | "visualize";

interface AgentFormProps {
  models?: ModelsFormData | null;
  databases?: WarehousesFormData | null;
}

export default function AgentForm({ models, databases }: AgentFormProps) {
  const {
    register,
    control,
    setValue,
    watch,
    formState: { errors }
  } = useFormContext<AgentConfig>();

  const {
    fields: toolFields,
    append: appendTool,
    remove: removeTool
  } = useFieldArray({
    control,
    name: "tools"
  });

  const tools = watch("tools") || [];

  const hasToolType = (type: ToolType): boolean => {
    return tools.some((tool) => tool.type === type);
  };

  const handleModelChange = (value: string) => {
    setValue("model", value, { shouldValidate: true });
  };

  const modelOptions =
    models?.models?.map((model) => {
      const modelName = model.name ? String(model.name) : "";
      return {
        value: modelName,
        label: `${model.vendor} - ${modelName}`
      };
    }) || [];

  const databasesOptions =
    databases?.warehouses?.map((db) => {
      const dbName = db.name ? String(db.name) : "";
      return {
        value: dbName,
        label: `${db.type} - ${dbName}`
      };
    }) || [];

  const handleDatabaseChange = (value: string, index: number) => {
    setValue(`tools.${index}.database`, value, { shouldValidate: true });
  };

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor='name'>Agent Name</Label>
        <Input
          id='name'
          placeholder='My Agent'
          {...register("name", {
            required: "Agent name is required"
          })}
        />
        {errors.name && (
          <p className='mt-1 text-destructive text-xs'>{errors.name.message?.toString()}</p>
        )}
        <p className='text-muted-foreground text-xs'>A unique name for your agent</p>
      </div>

      <div className='space-y-2'>
        <Label htmlFor='model'>Model</Label>
        <Select value={watch("model") || modelOptions[0]?.value} onValueChange={handleModelChange}>
          <SelectTrigger>
            <SelectValue placeholder='Select a model' />
          </SelectTrigger>
          <SelectContent>
            {modelOptions.map((option) => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        {errors.model && (
          <p className='mt-1 text-destructive text-xs'>{errors.model.message?.toString()}</p>
        )}
        <p className='text-muted-foreground text-xs'>The model your agent will use</p>
      </div>

      <div className='space-y-2'>
        <Label htmlFor='systemInstructions'>System Instructions</Label>
        <Textarea
          id='systemInstructions'
          placeholder='You are an AI assistant tasked with...'
          className='min-h-[150px]'
          {...register("system_instructions", {
            required: "System instructions are required"
          })}
        />
        {errors.system_instructions && (
          <p className='mt-1 text-destructive text-xs'>
            {errors.system_instructions.message?.toString()}
          </p>
        )}
        <p className='text-muted-foreground text-xs'>
          Instructions that define the agent's behavior
        </p>
      </div>

      <div className='space-y-2'>
        <Label>Description (Optional)</Label>
        <Textarea
          id='description'
          placeholder='This agent helps with...'
          {...register("description")}
        />
        <p className='text-muted-foreground text-xs'>A brief description of what the agent does</p>
      </div>

      <div className='space-y-2'>
        <Label>Agent Tools</Label>
        <div className='flex flex-col space-y-4'>
          {toolFields.map((field, index) => {
            const tool = watch(`tools.${index}`) as ToolConfig;
            return (
              <div key={field.id} className='rounded-md border p-3'>
                <div className='mb-3 flex items-center justify-between'>
                  <Badge>{tool.type}</Badge>
                  <Button
                    variant='ghost'
                    size='sm'
                    className='h-6 w-6 p-0'
                    onClick={() => removeTool(index)}
                    type='button'
                  >
                    <XIcon className='h-3 w-3' />
                  </Button>
                </div>
                <div className='space-y-3'>
                  <div className='space-y-2'>
                    <Label htmlFor={`tools.${index}.name`}>Name</Label>
                    <Input
                      id={`tools.${index}.name`}
                      placeholder={tool.type === "execute_sql" ? "SQL Executor" : "Data Visualizer"}
                      {...register(`tools.${index}.name`, {
                        pattern: {
                          value: /^\w+$/,
                          message:
                            "Tool name must start with a letter or underscore and contain only letters, numbers, and underscores"
                        }
                      })}
                    />
                    {errors.tools?.[index]?.name && (
                      <p className='mt-1 text-destructive text-xs'>
                        {errors.tools[index]?.name?.message?.toString()}
                      </p>
                    )}
                  </div>
                  <div className='space-y-2'>
                    <Label htmlFor={`tools.${index}.description`}>Description</Label>
                    <Input
                      id={`tools.${index}.description`}
                      placeholder={
                        tool.type === "execute_sql"
                          ? "Execute SQL queries against the database"
                          : "Create visualizations from data"
                      }
                      {...register(`tools.${index}.description`, {
                        required: "Tool description is required"
                      })}
                    />
                    {errors.tools?.[index]?.description && (
                      <p className='mt-1 text-destructive text-xs'>
                        {errors.tools[index]?.description?.message?.toString()}
                      </p>
                    )}
                  </div>
                  {tool.type === "execute_sql" && (
                    <div className='space-y-2'>
                      <Label htmlFor={`tools.${index}.database`}>Database</Label>
                      <Select
                        value={watch(`tools.${index}.database`) || databasesOptions[0]?.value}
                        onValueChange={(value) => handleDatabaseChange(value, index)}
                      >
                        <SelectTrigger>
                          <SelectValue placeholder='Select a database' />
                        </SelectTrigger>
                        <SelectContent>
                          {databasesOptions.map((option) => (
                            <SelectItem key={option.value} value={option.value}>
                              {option.label}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      {errors.tools?.[index] && (
                        <p className='mt-1 text-destructive text-xs'>
                          {(
                            errors.tools[index] as Record<string, { message?: string }>
                          )?.database?.message?.toString()}
                        </p>
                      )}
                    </div>
                  )}
                </div>
              </div>
            );
          })}

          <div className='flex flex-wrap gap-2'>
            {!hasToolType("execute_sql") && (
              <Button
                variant='outline'
                size='sm'
                className='gap-1'
                onClick={() => {
                  appendTool({
                    id: uuidv4(),
                    type: "execute_sql",
                    name: "",
                    description: "",
                    database: ""
                  } as ExecuteSQLToolConfig);
                }}
                type='button'
              >
                <PlusIcon className='h-3 w-3' /> ExecuteSQL Tool
              </Button>
            )}

            {!hasToolType("visualize") && (
              <Button
                variant='outline'
                size='sm'
                className='gap-1'
                onClick={() => {
                  appendTool({
                    id: uuidv4(),
                    type: "visualize",
                    name: "",
                    description: ""
                  });
                }}
                type='button'
              >
                <PlusIcon className='h-3 w-3' /> Visualize Tool
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
