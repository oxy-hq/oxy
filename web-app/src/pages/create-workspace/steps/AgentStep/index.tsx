import { Button } from "@/components/ui/shadcn/button";
import { FormProvider, useForm } from "react-hook-form";
import Header from "../Header";
import AgentForm from "./AgentForm";
import { ModelsFormData } from "../ModelStep";
import { WarehousesFormData } from "../WarehouseStep";
import { Loader2 } from "lucide-react";

export interface BaseToolConfig {
  id: string;
  name: string;
  description: string;
}

export interface ExecuteSQLToolConfig extends BaseToolConfig {
  type: "execute_sql";
  database: string;
}

export interface VisualizeToolConfig extends BaseToolConfig {
  type: "visualize";
}

export type ToolConfig = ExecuteSQLToolConfig | VisualizeToolConfig;

export interface AgentConfig {
  name: string;
  model: string;
  system_instructions: string;
  description?: string;
  public?: boolean;
  tools?: ToolConfig[];
}

interface AgentStepProps {
  isCreating: boolean;
  initialData?: AgentConfig | null;
  models?: ModelsFormData | null;
  databases?: WarehousesFormData | null;
  onNext: (data: AgentConfig) => void;
  onBack: () => void;
}

export default function AgentStep({
  isCreating,
  initialData,
  models,
  databases,
  onNext,
  onBack,
}: AgentStepProps) {
  const methods = useForm<AgentConfig>({
    defaultValues: initialData || {
      name: "default-agent",
      model: models?.models?.[0].name,
      system_instructions:
        "You are an Data Analyst expert.\nYour task is to help the user generate report given the input.\nONLY use the provided data from user's input.\nFollow best practices to generate the report.",
      description:
        "An agent for anonymizing sensitive data and generating reports.",
      tools: [
        {
          id: "visualize-tool",
          name: "generate_chart",
          description:
            "Render a chart based on the data provided, make sure to use the correct chart type and fields.",
          type: "visualize",
        },
        {
          id: "execute-sql-tool",
          name: "execute_sql",
          description:
            "Execute the SQL query. If the query is invalid, fix it and run again.",
          type: "execute_sql",
          database: databases?.warehouses?.[0].name,
        },
      ],
    },
  });

  const {
    handleSubmit,
    formState: { isValid },
  } = methods;

  const onSubmit = (data: AgentConfig) => {
    onNext(data);
  };

  return (
    <FormProvider {...methods}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-6">
        <div className="space-y-4">
          <Header
            title="Configure agent"
            description="Set up an agent for your workspace."
          />

          <AgentForm models={models} databases={databases} />
        </div>

        <div className="flex justify-between">
          <Button type="button" variant="outline" onClick={onBack}>
            Back
          </Button>
          <Button type="submit" disabled={!isValid || isCreating}>
            {isCreating && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Create Workspace
          </Button>
        </div>
      </form>
    </FormProvider>
  );
}
