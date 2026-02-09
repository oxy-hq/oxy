import { Plus } from "lucide-react";
import { useEffect } from "react";
import { Controller, FormProvider, useFieldArray, useForm } from "react-hook-form";
import { TestsForm } from "@/components/shared/TestsForm";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
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
import { cleanObject } from "@/utils/formDataCleaner";
import { DefaultAgentForm } from "./DefaultAgentForm";
import { ReasoningForm } from "./ReasoningForm";
import { RetrievalForm } from "./RetrievalForm";
import { RoutingForm } from "./RoutingForm";

export interface AgentFormData {
  name?: string;
  model?: string;
  description?: string;
  public?: boolean;
  agent_type?: "default" | "routing";
  system_instructions?: string;
  max_tool_calls?: number;
  max_tool_concurrency?: number;
  context?: ContextFormData[];
  tools?: ToolFormData[];
  tests?: TestFormData[];
  retrieval?: RetrievalConfigData | null;
  reasoning?: ReasoningConfigData | null;
  // Routing agent specific
  routes?: string[];
  route_fallback?: string;
  embed_model?: string;
  top_k?: number;
  factor?: number;
  n_dims?: number;
  synthesize_results?: boolean;
}

export interface ContextFormData {
  name?: string;
  type?: string;
  src?: string | string[];
}

export interface ToolFormData {
  type?: string;
  name?: string;
  description?: string;
  [key: string]: unknown;
}

export interface TestFormData {
  type?: string;
  concurrency?: number;
  task_ref?: string;
  metrics?: unknown[];
  [key: string]: unknown;
}

export interface RetrievalConfigData {
  include?: string[];
  exclude?: string[];
}

export interface ReasoningConfigData {
  effort?: "low" | "medium" | "high";
}

interface AgentFormProps {
  data?: Partial<AgentFormData>;
  onChange?: (data: Partial<AgentFormData>) => void;
}

const cleanFormData = (data: Partial<AgentFormData>): Partial<AgentFormData> => {
  return (cleanObject(data as Record<string, unknown>) as Partial<AgentFormData>) || {};
};

// eslint-disable-next-line sonarjs/cognitive-complexity
const getDefaultData = (data?: Partial<AgentFormData>) => {
  // Determine agent type from data
  const isRoutingAgent = data?.routes && Array.isArray(data.routes) && data.routes.length > 0;
  const agentType = isRoutingAgent ? "routing" : "default";

  if (!data) {
    return {
      name: "",
      model: "",
      description: "",
      public: true,
      agent_type: "default" as const,
      system_instructions: "",
      max_tool_calls: 10,
      max_tool_concurrency: 10,
      context: [],
      tools: [],
      tests: [],
      retrieval: null,
      reasoning: null
    };
  }

  const result: Partial<AgentFormData> = {};

  if (data.name !== undefined) result.name = data.name;
  if (data.model !== undefined) result.model = data.model;
  if (data.description !== undefined) result.description = data.description;
  if (data.public !== undefined) result.public = data.public;
  else result.public = true;

  result.agent_type = agentType;

  if (data.system_instructions !== undefined) result.system_instructions = data.system_instructions;
  if (data.max_tool_calls !== undefined) result.max_tool_calls = data.max_tool_calls;
  else if (agentType === "default") result.max_tool_calls = 10;
  if (data.max_tool_concurrency !== undefined)
    result.max_tool_concurrency = data.max_tool_concurrency;
  else if (agentType === "default") result.max_tool_concurrency = 10;

  if (data.context && Array.isArray(data.context) && data.context.length > 0) {
    result.context = data.context;
  }
  if (data.tools && Array.isArray(data.tools) && data.tools.length > 0) {
    result.tools = data.tools;
  }
  if (data.tests && Array.isArray(data.tests) && data.tests.length > 0) {
    result.tests = data.tests;
  }

  if (data.retrieval && typeof data.retrieval === "object") {
    const hasInclude =
      data.retrieval.include &&
      Array.isArray(data.retrieval.include) &&
      data.retrieval.include.length > 0;
    const hasExclude =
      data.retrieval.exclude &&
      Array.isArray(data.retrieval.exclude) &&
      data.retrieval.exclude.length > 0;
    if (hasInclude || hasExclude) {
      result.retrieval = data.retrieval;
    }
  }

  if (data.reasoning && typeof data.reasoning === "object") {
    result.reasoning = data.reasoning;
  }

  // Routing agent fields
  if (data.routes && Array.isArray(data.routes) && data.routes.length > 0) {
    result.routes = data.routes;
  }
  if (data.route_fallback !== undefined) result.route_fallback = data.route_fallback;
  if (data.embed_model !== undefined) result.embed_model = data.embed_model;
  if (data.top_k !== undefined) result.top_k = data.top_k;
  if (data.factor !== undefined) result.factor = data.factor;
  if (data.n_dims !== undefined) result.n_dims = data.n_dims;
  if (data.synthesize_results !== undefined) result.synthesize_results = data.synthesize_results;

  return result;
};

export const AgentForm: React.FC<AgentFormProps> = ({ data, onChange }) => {
  const methods = useForm<AgentFormData>({
    defaultValues: getDefaultData(data),
    mode: "onBlur"
  });

  const { watch } = methods;

  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  useEffect(() => {
    const subscription = watch((value) => {
      if (methods.formState.isDirty) {
        const cleaned = cleanFormData(value as Partial<AgentFormData>);
        onChange?.(cleaned);
      }
    });
    return () => subscription.unsubscribe();
  }, [watch, onChange]);

  const {
    control,
    register,
    formState: { errors }
  } = methods;

  const {
    fields: testFields,
    append: appendTest,
    remove: removeTest
  } = useFieldArray({
    control,
    name: "tests"
  });

  // Watch agent type
  const agentType = watch("agent_type");
  const isRoutingAgent = agentType === "routing";

  return (
    <FormProvider {...methods}>
      <div className='flex min-h-0 flex-1 flex-col bg-card'>
        <div className='customScrollbar flex-1 overflow-auto p-6'>
          <form id='agent-form' className='space-y-8'>
            {/* Basic fields */}
            <div className='space-y-4'>
              <div className='space-y-2'>
                <Label htmlFor='name'>Name</Label>
                <Input
                  id='name'
                  placeholder='Agent name (e.g., sql-generator, data-analyst)'
                  {...register("name")}
                />
                {errors.name && <p className='text-red-500 text-sm'>{errors.name.message}</p>}
              </div>

              <div className='space-y-2'>
                <Label htmlFor='agent_type'>Agent Type *</Label>
                <Controller
                  name='agent_type'
                  control={control}
                  rules={{ required: "Agent type is required" }}
                  render={({ field }) => (
                    <Select onValueChange={field.onChange} value={field.value}>
                      <SelectTrigger>
                        <SelectValue placeholder='Select agent type' />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value='default'>Default Agent</SelectItem>
                        <SelectItem value='routing'>Routing Agent</SelectItem>
                      </SelectContent>
                    </Select>
                  )}
                />
                {errors.agent_type && (
                  <p className='text-red-500 text-sm'>{errors.agent_type.message}</p>
                )}
                <p className='text-muted-foreground text-sm'>
                  {agentType === "routing"
                    ? "Routing agent routes queries to other agents based on semantic similarity"
                    : "Default agent executes tasks using configured tools and system instructions"}
                </p>
              </div>

              <div className='space-y-2'>
                <Label htmlFor='model'>Model *</Label>
                <Input
                  id='model'
                  placeholder='e.g., gpt-4o, claude-3-5-sonnet-20241022'
                  {...register("model", { required: "Model is required" })}
                />
                {errors.model && <p className='text-red-500 text-sm'>{errors.model.message}</p>}
              </div>

              <div className='space-y-2'>
                <Label htmlFor='description'>Description</Label>
                <Textarea
                  id='description'
                  placeholder='Describe what this agent does...'
                  {...register("description")}
                  rows={3}
                />
              </div>

              <div className='flex items-center space-x-2'>
                <input type='checkbox' id='public' {...register("public")} className='rounded' />
                <Label htmlFor='public'>Public</Label>
              </div>
            </div>

            {/* Conditional rendering based on agent type */}
            {isRoutingAgent ? <RoutingForm /> : <DefaultAgentForm />}

            <ReasoningForm />

            <div className='flex items-center justify-between'>
              <CardTitle>Tests</CardTitle>
              <Button
                type='button'
                onClick={() =>
                  appendTest({
                    type: "consistency",
                    concurrency: 10
                  })
                }
                variant='outline'
                size='sm'
              >
                <Plus className='mr-2 h-4 w-4' />
                Add Test
              </Button>
            </div>
            <div className='space-y-4'>
              {testFields.map((field, index) => (
                <TestsForm<AgentFormData>
                  key={field.id}
                  index={index}
                  onRemove={() => removeTest(index)}
                />
              ))}
            </div>

            <RetrievalForm />
          </form>
        </div>
      </div>
    </FormProvider>
  );
};
