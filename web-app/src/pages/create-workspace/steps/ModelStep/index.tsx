import { Plus, Trash2 } from "lucide-react";
import type React from "react";
import { FormProvider, useFieldArray, useForm } from "react-hook-form";
import { AnthropicIcon, GoogleIcon, OllamaIcon, OpenAiIcon } from "@/components/icons";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { cn } from "@/libs/utils/cn";
import Header from "../Header";
import AnthropicForm from "./AnthropicForm";
import GoogleForm from "./GoogleForm";
import OllamaForm from "./OllamaForm";
import OpenAIForm from "./OpenAIForm";

export type ModelVendor = "openai" | "google" | "anthropic" | "ollama";

interface ModelOption {
  vendor: ModelVendor;
  label: string;
  icon?: React.ReactNode;
}

export type ModelConfig = {
  vendor: ModelVendor;
  name?: string;
  config:
    | OpenAIModelConfig
    | GoogleModelConfig
    | AnthropicModelConfig
    | OllamaModelConfig
    | Record<string, unknown>;
};

export interface AnthropicModelConfig {
  model_ref: string;
  api_key: string;
  api_url?: string;
}

export interface OllamaModelConfig {
  model_ref: string;
  api_key: string;
  api_url: string;
}

export interface GoogleModelConfig {
  model_ref: string;
  api_key: string;
  api_url?: string;
  project_id?: string;
}

export interface OpenAIModelConfig {
  model_ref: string;
  api_key: string;
  api_url?: string;
}

export interface ModelsFormData {
  models: ModelConfig[];
}

const modelOptions: ModelOption[] = [
  {
    vendor: "openai",
    label: "OpenAI",
    icon: <OpenAiIcon />
  },
  {
    vendor: "google",
    label: "Google",
    icon: <GoogleIcon />
  },
  {
    vendor: "anthropic",
    label: "Anthropic",
    icon: <AnthropicIcon />
  },
  {
    vendor: "ollama",
    label: "Ollama",
    icon: <OllamaIcon />
  }
];

interface ModelStepProps {
  initialData?: ModelsFormData | null;
  onNext: (data: ModelsFormData) => void;
  onBack: () => void;
}

export default function ModelStep({ initialData, onNext, onBack }: ModelStepProps) {
  const methods = useForm<ModelsFormData>({
    defaultValues: initialData || {
      models: [
        {
          vendor: "openai",
          name: "OPENAI_1",
          config: {}
        }
      ]
    }
  });

  const { register, control, handleSubmit, watch, setValue } = methods;

  const { fields, append, remove } = useFieldArray({
    control,
    name: "models"
  });

  const handleVendorChange = (index: number, value: ModelVendor) => {
    setValue(`models.${index}.vendor`, value);
    setValue(`models.${index}.config`, {});
  };

  const onSubmit = (data: ModelsFormData) => {
    onNext(data);
  };

  const renderModelForm = (index: number, modelVendor: ModelVendor) => {
    switch (modelVendor) {
      case "openai":
        return <OpenAIForm index={index} />;
      case "google":
        return <GoogleForm index={index} />;
      case "anthropic":
        return <AnthropicForm index={index} />;
      case "ollama":
        return <OllamaForm index={index} />;
      default:
        return null;
    }
  };

  return (
    <FormProvider {...methods}>
      <form onSubmit={handleSubmit(onSubmit)} className='space-y-6'>
        <div className='space-y-6'>
          <Header title='Add models' description='Configure your AI model providers.' />

          {fields.map((field, index) => {
            const modelVendor = watch(`models.${index}.vendor`) as ModelVendor;
            const defaultName = `${modelVendor.toUpperCase()}_${index + 1}`;

            const nameError = methods.formState.errors?.models?.[index]?.name;

            return (
              <div
                key={field.id}
                className='mb-4 flex flex-col gap-4 rounded-md border bg-muted/40 p-3'
              >
                <div>
                  <div className='flex flex-row items-center justify-between gap-4'>
                    <Input
                      {...register(`models.${index}.name`, {
                        required: "Model name is required",
                        pattern: {
                          value: /^\w+$/,
                          message: "Model name must be alphanumeric and can include underscores"
                        },
                        validate: {
                          unique: (value) => {
                            const modelNames = methods
                              .getValues("models")
                              .map((model, i) => (i !== index ? model.name : null));
                            return !modelNames.includes(value) || "Model name must be unique";
                          }
                        }
                      })}
                      defaultValue={defaultName}
                      placeholder='OPEN_AI_GPT4'
                    />
                    {fields.length > 1 && (
                      <Button
                        type='button'
                        variant='ghost'
                        size='icon'
                        onClick={() => remove(index)}
                        className='h-8 w-8'
                      >
                        <Trash2 className='h-4 w-4' />
                      </Button>
                    )}
                  </div>
                  {nameError && (
                    <p className='mt-1 text-destructive text-xs'>{nameError.message?.toString()}</p>
                  )}
                </div>

                <div className='flex flex-wrap gap-4'>
                  {modelOptions.map((option) => (
                    <Button
                      key={option.vendor}
                      type='button'
                      variant='outline'
                      size='icon'
                      onClick={() => handleVendorChange(index, option.vendor)}
                      className={cn("px-8 py-4", modelVendor === option.vendor && "border-primary")}
                    >
                      {option.icon}
                    </Button>
                  ))}
                </div>

                {renderModelForm(index, modelVendor)}
              </div>
            );
          })}

          <Button
            type='button'
            variant='outline'
            size='sm'
            onClick={() =>
              append({
                vendor: "openai",
                name: `OPENAI_${fields.length + 1}`,
                config: {}
              })
            }
            className='w-full'
          >
            <Plus className='h-4 w-4' /> Add Another Model
          </Button>
        </div>

        <div className='flex justify-between'>
          <Button type='button' variant='outline' onClick={onBack}>
            Back
          </Button>
          <Button type='submit'>Next</Button>
        </div>
      </form>
    </FormProvider>
  );
}
