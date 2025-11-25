import React, { useState } from "react";
import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { Button } from "@/components/ui/shadcn/button";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Plus, X } from "lucide-react";
import { WorkflowFormData } from "../..";

interface ExecuteSqlTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const ExecuteSqlTaskFields: React.FC<ExecuteSqlTaskFieldsProps> = ({
  index,
  basePath = "tasks",
}) => {
  const {
    register,
    watch,
    setValue,
    formState: { errors },
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];
  // @ts-expect-error - Dynamic path for nested tasks
  const variables = watch(`${taskPath}.variables`) || {};
  const [variableEntries, setVariableEntries] = useState<
    Array<{ key: string; value: string }>
  >(
    Object.entries(variables).map(([key, value]) => ({
      key,
      value: String(value),
    })),
  );

  const addVariable = () => {
    const newEntries = [...variableEntries, { key: "", value: "" }];
    setVariableEntries(newEntries);
  };

  const removeVariable = (indexToRemove: number) => {
    const newEntries = variableEntries.filter((_, i) => i !== indexToRemove);
    setVariableEntries(newEntries);
    updateVariables(newEntries);
  };

  const updateVariableKey = (varIndex: number, key: string) => {
    const newEntries = [...variableEntries];
    newEntries[varIndex].key = key;
    setVariableEntries(newEntries);
    updateVariables(newEntries);
  };

  const updateVariableValue = (varIndex: number, value: string) => {
    const newEntries = [...variableEntries];
    newEntries[varIndex].value = value;
    setVariableEntries(newEntries);
    updateVariables(newEntries);
  };

  const updateVariables = (entries: Array<{ key: string; value: string }>) => {
    const variablesObj: Record<string, unknown> = {};
    entries.forEach((entry) => {
      if (entry.key) {
        try {
          variablesObj[entry.key] = JSON.parse(entry.value);
        } catch {
          variablesObj[entry.key] = entry.value;
        }
      }
    });
    setValue(
      // @ts-expect-error - Dynamic path for nested tasks
      `${taskPath}.variables`,
      Object.keys(variablesObj).length > 0 ? variablesObj : undefined,
    );
  };

  return (
    <div className="space-y-4">
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.database`}>Database</Label>
        <Input
          id={`${taskPath}.database`}
          placeholder="Enter database name"
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.database`, {
            required: "Database is required",
          })}
        />
        {taskErrors?.database && (
          <p className="text-sm text-red-500">{taskErrors.database.message}</p>
        )}
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.sql_query`}>SQL Query</Label>
        <Textarea
          id={`${taskPath}.sql_query`}
          placeholder="Enter SQL query or leave empty if using sql_file"
          rows={4}
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.sql_query`)}
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.sql_file`}>SQL File Path</Label>
        <FilePathAutocompleteInput
          id={`${taskPath}.sql_file`}
          fileExtension=".sql"
          datalistId={`sql-files-${basePath}-${index}`}
          placeholder="Enter path to SQL file or leave empty if using sql_query"
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.sql_file`)}
        />
        <p className="text-sm text-muted-foreground">
          Or leave empty if using sql_query above
        </p>
      </div>
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <Label>Variables</Label>
          <Button
            type="button"
            onClick={addVariable}
            variant="outline"
            size="sm"
          >
            <Plus className="w-4 h-4 mr-1" />
            Add Variable
          </Button>
        </div>
        {variableEntries.length > 0 && (
          <div className="space-y-2">
            {variableEntries.map((entry, varIndex) => (
              <div key={varIndex} className="flex gap-2 items-center">
                <div className="flex-1">
                  <Input
                    placeholder="Variable name"
                    value={entry.key}
                    onChange={(e) =>
                      updateVariableKey(varIndex, e.target.value)
                    }
                  />
                </div>
                <div className="flex-1">
                  <Input
                    placeholder="Variable value"
                    value={entry.value}
                    onChange={(e) =>
                      updateVariableValue(varIndex, e.target.value)
                    }
                  />
                </div>
                <Button
                  className="text-destructive"
                  type="button"
                  onClick={() => removeVariable(varIndex)}
                  variant="ghost"
                  size="sm"
                >
                  <X className="w-4 h-4 text-destructive!" />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className="text-sm text-muted-foreground">
          Add variables to use in your SQL query as placeholders (e.g.,{" "}
          {"{{ variable_name }}"})
        </p>
      </div>
      <div className="space-y-2">
        <Label htmlFor={`${taskPath}.dry_run_limit`}>Dry Run Limit</Label>
        <Input
          id={`${taskPath}.dry_run_limit`}
          type="number"
          min="0"
          placeholder="Optional limit for dry run"
          // @ts-expect-error - Dynamic field path
          {...register(`${taskPath}.dry_run_limit`, {
            valueAsNumber: true,
          })}
        />
      </div>
    </div>
  );
};
