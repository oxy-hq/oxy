import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import React, { useCallback, useState } from "react";
import { useFieldArray, useForm } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { CardTitle } from "@/components/ui/shadcn/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";

export interface TestFileFormData {
  name?: string | null;
  target?: string | null;
  settings: {
    concurrency: number;
    runs: number;
    judge_model?: string | null;
  };
  cases: Array<{
    prompt: string;
    expected: string;
    tags: string[];
    tool?: string | null;
  }>;
}

interface TestFileFormProps {
  data: Partial<TestFileFormData>;
  onChange: (data: TestFileFormData) => void;
}

interface CaseItemProps {
  index: number;
  register: ReturnType<typeof useForm<TestFileFormData>>["register"];
  onRemove: () => void;
  prompt: string;
}

const CaseItem: React.FC<CaseItemProps> = ({ index, register, onRemove, prompt }) => {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className='rounded-lg border p-3'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='w-full rounded-lg'>
          <div className='flex min-w-0 items-center justify-between'>
            {isOpen ? (
              <ChevronDown className='h-5 w-5 text-muted-foreground' />
            ) : (
              <ChevronRight className='h-5 w-5 text-muted-foreground' />
            )}
            <div className='flex min-w-0 flex-1 items-center gap-3'>
              <span className='flex h-8 w-8 items-center justify-center rounded-full bg-primary/10 font-semibold text-primary text-sm'>
                {index + 1}
              </span>
              <div className='flex min-w-0 flex-1 items-center gap-2'>
                <span className='truncate font-medium text-sm'>
                  {prompt || "Untitled Case"}
                </span>
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
              <Label htmlFor={`case-${index}-prompt`}>Prompt</Label>
              <Textarea
                id={`case-${index}-prompt`}
                placeholder='Enter prompt...'
                rows={2}
                {...register(`cases.${index}.prompt`)}
              />
            </div>
            <div className='space-y-2'>
              <Label htmlFor={`case-${index}-expected`}>Expected Answer</Label>
              <Textarea
                id={`case-${index}-expected`}
                placeholder='Enter expected answer...'
                rows={3}
                {...register(`cases.${index}.expected`)}
              />
            </div>
            <div className='grid grid-cols-2 gap-4'>
              <div className='space-y-2'>
                <Label htmlFor={`case-${index}-tags`}>Tags (comma-separated)</Label>
                <Input
                  id={`case-${index}-tags`}
                  placeholder='e.g. smoke, regression'
                  {...register(`cases.${index}.tags`, {
                    setValueAs: (v: string | string[]) =>
                      typeof v === "string"
                        ? v
                            .split(",")
                            .map((t) => t.trim())
                            .filter(Boolean)
                        : v
                  })}
                />
              </div>
              <div className='space-y-2'>
                <Label htmlFor={`case-${index}-tool`}>Tool (optional)</Label>
                <Input
                  id={`case-${index}-tool`}
                  placeholder='e.g. search_tool'
                  {...register(`cases.${index}.tool`)}
                />
              </div>
            </div>
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

const TestFileForm: React.FC<TestFileFormProps> = ({ data, onChange }) => {
  const { register, control, watch, subscribe } = useForm<TestFileFormData>({
    defaultValues: {
      name: data.name ?? null,
      target: data.target ?? null,
      settings: {
        concurrency: data.settings?.concurrency ?? 5,
        runs: data.settings?.runs ?? 3,
        judge_model: data.settings?.judge_model ?? null
      },
      cases: data.cases ?? []
    }
  });

  const { fields, append, remove } = useFieldArray({
    control,
    name: "cases"
  });

  // Use subscribe with isDirty (same pattern as WorkflowForm) to avoid
  // triggering onChange on initial load / resets
  React.useEffect(() => {
    const callback = subscribe({
      formState: {
        values: true,
        isDirty: true
      },
      callback: ({ values, isDirty }) => {
        if (isDirty) {
          onChange(values as TestFileFormData);
        }
      }
    });
    return () => callback();
  }, [subscribe, onChange]);

  const cases = watch("cases");

  const handleAddCase = useCallback(() => {
    append({ prompt: "", expected: "", tags: [], tool: null });
  }, [append]);

  return (
    <div className='flex min-h-0 flex-1 flex-col'>
      <div className='customScrollbar flex-1 overflow-auto p-4'>
        <form className='space-y-8'>
          <div className='space-y-4'>
            <div className='space-y-2'>
              <Label htmlFor='name'>Display Name</Label>
              <Input
                id='name'
                placeholder='e.g. Restaurant Analyst Tests'
                {...register("name")}
              />
            </div>
            <div className='space-y-2'>
              <Label htmlFor='target'>Target</Label>
              <Input
                id='target'
                placeholder='e.g. sales.agent.yml'
                {...register("target")}
              />
            </div>
            <div className='grid grid-cols-2 gap-4'>
              <div className='space-y-2'>
                <Label htmlFor='concurrency'>Concurrency</Label>
                <Input
                  id='concurrency'
                  type='number'
                  min={1}
                  {...register("settings.concurrency", { valueAsNumber: true })}
                />
              </div>
              <div className='space-y-2'>
                <Label htmlFor='runs'>Runs</Label>
                <Input
                  id='runs'
                  type='number'
                  min={1}
                  {...register("settings.runs", { valueAsNumber: true })}
                />
              </div>
            </div>
            <div className='space-y-2'>
              <Label htmlFor='judge_model'>Judge Model</Label>
              <Input
                id='judge_model'
                placeholder='e.g. gpt-4o'
                {...register("settings.judge_model")}
              />
            </div>
          </div>

          <div className='space-y-4'>
            <div className='flex items-center justify-between'>
              <CardTitle>Cases</CardTitle>
              <Button type='button' onClick={handleAddCase} variant='outline' size='sm'>
                <Plus />
                Add Case
              </Button>
            </div>

            {fields.length === 0 && (
              <div className='rounded-lg border-2 border-muted-foreground/25 border-dashed p-6 text-center'>
                <p className='text-muted-foreground text-sm'>
                  No test cases yet. Click "Add Case" to get started.
                </p>
              </div>
            )}

            <div className='space-y-4'>
              {fields.map((field, index) => (
                <CaseItem
                  key={field.id}
                  index={index}
                  register={register}
                  onRemove={() => remove(index)}
                  prompt={cases?.[index]?.prompt ?? ""}
                />
              ))}
            </div>
          </div>
        </form>
      </div>
    </div>
  );
};

export default TestFileForm;
