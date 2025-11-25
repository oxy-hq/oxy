import React, { useState } from "react";
import { useFormContext, Controller } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Switch } from "@/components/ui/shadcn/switch";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/shadcn/collapsible";
import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { WorkflowFormData } from "..";
import {
  AgentTaskFields,
  ExecuteSqlTaskFields,
  SemanticQueryTaskFields,
  OmniQueryTaskFields,
  FormatterTaskFields,
  WorkflowTaskFields,
  LoopSequentialTaskFields,
  ConditionalTaskFields,
} from "./TaskFields";

interface TaskFormProps {
  index: number;
  onRemove: () => void;
  /**
   * Base path for the tasks array (e.g., "tasks" or "tasks.0.tasks")
   */
  basePath?: string;
}

const TASK_TYPES = [
  { value: "agent", label: "Agent" },
  { value: "execute_sql", label: "Execute SQL" },
  { value: "semantic_query", label: "Semantic Query" },
  { value: "omni_query", label: "Omni Query" },
  { value: "loop_sequential", label: "Loop Sequential" },
  { value: "formatter", label: "Formatter" },
  { value: "workflow", label: "Workflow" },
  { value: "conditional", label: "Conditional" },
];

const EXPORT_FORMATS = [
  { value: "sql", label: "SQL" },
  { value: "csv", label: "CSV" },
  { value: "json", label: "JSON" },
  { value: "txt", label: "Text" },
  { value: "docx", label: "Word Document" },
];

export const TaskForm: React.FC<TaskFormProps> = ({
  index,
  onRemove,
  basePath = "tasks",
}) => {
  const {
    register,
    control,
    watch,
    formState: { errors },
    setValue,
    getValues,
  } = useFormContext<WorkflowFormData>();

  const [isOpen, setIsOpen] = useState(false);

  // Build the full path for this task (e.g., "tasks.0" or "tasks.0.tasks.1")
  const taskPath = `${basePath}.${index}` as const;

  // @ts-expect-error - Dynamic path for nested tasks
  const taskType = watch(`${taskPath}.type`) as string | undefined;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskName = watch(`${taskPath}.name`) as string | undefined;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  const getTaskTypeLabel = (type: string) => {
    const taskTypeObj = TASK_TYPES.find((t) => t.value === type);
    return taskTypeObj?.label || type;
  };

  const onChangeTaskType = (newType: string) => {
    // @ts-expect-error - Dynamic path for nested tasks
    const task = getValues(taskPath) as Record<string, unknown>;
    if (newType !== taskType) {
      // @ts-expect-error - Dynamic path for nested tasks
      setValue(taskPath, {
        name: task?.name,
        type: task?.type,
        export: task?.export,
        cache: task?.cache,
      });
    }
  };

  const renderTaskSpecificFields = () => {
    switch (taskType) {
      case "agent":
        return <AgentTaskFields index={index} basePath={basePath} />;

      case "execute_sql":
        return <ExecuteSqlTaskFields index={index} basePath={basePath} />;

      case "semantic_query":
        return <SemanticQueryTaskFields index={index} basePath={basePath} />;

      case "omni_query":
        return <OmniQueryTaskFields index={index} basePath={basePath} />;

      case "formatter":
        return <FormatterTaskFields index={index} basePath={basePath} />;

      case "workflow":
        return <WorkflowTaskFields index={index} basePath={basePath} />;

      case "loop_sequential":
        return <LoopSequentialTaskFields index={index} basePath={basePath} />;

      case "conditional":
        return <ConditionalTaskFields index={index} basePath={basePath} />;

      default:
        return null;
    }
  };

  return (
    <div className="rounded-lg border bg-card p-3">
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className="rounded-lg w-full">
          <div className="min-w-0 flex items-center justify-between">
            {isOpen ? (
              <ChevronDown className="h-5 w-5 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-5 w-5 text-muted-foreground" />
            )}
            <div className="min-w-0 flex items-center gap-3 flex-1">
              <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary font-semibold text-sm">
                {index + 1}
              </span>
              <div className="min-w-0 flex items-center gap-2 flex-1">
                <span className="truncate font-medium text-sm">
                  {taskName || "Untitled Task"}
                </span>
                {taskType && (
                  <span className="text-xs px-2 py-1 rounded-md bg-muted text-muted-foreground">
                    {getTaskTypeLabel(taskType)}
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
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor={`${taskPath}.name`}>Task Name</Label>
                <Input
                  id={`${taskPath}.name`}
                  placeholder="Enter task name"
                  // @ts-expect-error - Dynamic path for nested tasks
                  {...register(`${taskPath}.name`, {
                    required: "Task name is required",
                    pattern: {
                      value: /^[a-zA-Z]\w*$/,
                      message:
                        "Name must start with a letter and contain only alphanumeric characters and underscores",
                    },
                  })}
                />
                {taskErrors?.name && (
                  <p className="text-sm text-red-500">
                    {taskErrors.name.message}
                  </p>
                )}
              </div>
              <div className="space-y-2">
                <Label htmlFor={`${taskPath}.type`}>Task Type</Label>
                <Controller
                  // @ts-expect-error - Dynamic path for nested tasks
                  name={`${taskPath}.type`}
                  control={control}
                  rules={{ required: "Task type is required" }}
                  render={({ field }) => (
                    <Select
                      onValueChange={(value) => {
                        field.onChange(value);
                        onChangeTaskType(value);
                      }}
                      defaultValue={field.value as string}
                    >
                      <SelectTrigger>
                        <SelectValue placeholder="Select task type" />
                      </SelectTrigger>
                      <SelectContent>
                        {TASK_TYPES.map((type) => (
                          <SelectItem key={type.value} value={type.value}>
                            {type.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />
                {taskErrors?.type && (
                  <p className="text-sm text-red-500">
                    {taskErrors.type.message}
                  </p>
                )}
              </div>
            </div>

            {renderTaskSpecificFields()}

            <div className="space-y-4 border-t pt-4">
              <h4 className="font-medium">Cache Configuration</h4>
              <div className="flex items-center space-x-2">
                <Controller
                  // @ts-expect-error - Dynamic path for nested tasks
                  name={`${taskPath}.cache.enabled`}
                  control={control}
                  render={({ field: { value, onChange } }) => (
                    <Switch
                      id={`${taskPath}.cache.enabled`}
                      checked={(value as boolean) || false}
                      onCheckedChange={onChange}
                    />
                  )}
                />
                <Label htmlFor={`${taskPath}.cache.enabled`}>
                  Enable caching
                </Label>
              </div>
              {/* @ts-expect-error - Dynamic path for nested tasks */}
              {watch(`${taskPath}.cache.enabled`) && (
                <div className="space-y-2">
                  <Label htmlFor={`${taskPath}.cache.path`}>Cache Path</Label>
                  <FilePathAutocompleteInput
                    id={`${taskPath}.cache.path`}
                    datalistId={`cache-path-${basePath}-${index}`}
                    placeholder="Enter cache file path"
                    // @ts-expect-error - Dynamic path for nested tasks
                    {...register(`${taskPath}.cache.path`)}
                  />
                </div>
              )}
            </div>

            <div className="space-y-4 border-t pt-4">
              <h4 className="font-medium">Export Configuration</h4>
              <div className="flex items-center space-x-2">
                <Controller
                  // @ts-expect-error - Dynamic path for nested tasks
                  name={`${taskPath}.export.enabled`}
                  control={control}
                  render={({ field: { value, onChange } }) => (
                    <Switch
                      id={`${taskPath}.export.enabled`}
                      checked={(value as boolean) || false}
                      onCheckedChange={onChange}
                    />
                  )}
                />
                <Label htmlFor={`${taskPath}.export.enabled`}>
                  Enable export
                </Label>
              </div>
              {/* @ts-expect-error - Dynamic path for nested tasks */}
              {watch(`${taskPath}.export.enabled`) && (
                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <Label htmlFor={`${taskPath}.export.format`}>
                      Export Format
                    </Label>
                    <Controller
                      // @ts-expect-error - Dynamic path for nested tasks
                      name={`${taskPath}.export.format`}
                      control={control}
                      render={({ field }) => (
                        <Select
                          onValueChange={field.onChange}
                          value={(field.value as string) || ""}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="Select format" />
                          </SelectTrigger>
                          <SelectContent>
                            {EXPORT_FORMATS.map((format) => (
                              <SelectItem
                                key={format.value}
                                value={format.value}
                              >
                                {format.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      )}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label htmlFor={`${taskPath}.export.path`}>
                      Export Path
                    </Label>
                    <FilePathAutocompleteInput
                      id={`${taskPath}.export.path`}
                      datalistId={`export-path-${basePath}-${index}`}
                      placeholder="Enter export file path"
                      // @ts-expect-error - Dynamic path for nested tasks
                      {...register(`${taskPath}.export.path`)}
                    />
                  </div>
                </div>
              )}
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
