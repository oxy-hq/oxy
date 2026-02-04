import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Controller, useFormContext } from "react-hook-form";
import { FilePathAutocompleteInput } from "@/components/ui/FilePathAutocompleteInput";
import { Button } from "@/components/ui/shadcn/button";
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

interface TestsFormProps {
  index: number;
  onRemove: () => void;
}

const TEST_TYPES = [
  { value: "consistency", label: "Consistency" },
  { value: "custom", label: "Custom" }
];

export const TestsForm: React.FC<TestsFormProps> = ({ index, onRemove }) => {
  const {
    register,
    control,
    watch,
    formState: { errors }
  } = useFormContext<AgentFormData>();

  const [isOpen, setIsOpen] = useState(false);
  const testType = watch(`tests.${index}.type`);
  const testErrors = errors.tests?.[index];

  const getTestTypeLabel = (type: string) => {
    const testTypeObj = TEST_TYPES.find((t) => t.value === type);
    return testTypeObj?.label || type;
  };

  const renderTestSpecificFields = () => {
    switch (testType) {
      case "consistency":
        return (
          <div className='space-y-4'>
            <div className='space-y-2'>
              <Label htmlFor={`tests.${index}.n`}>Number of runs (n)</Label>
              <Input
                id={`tests.${index}.n`}
                type='number'
                min='1'
                defaultValue={10}
                {...register(`tests.${index}.n`, {
                  valueAsNumber: true
                })}
              />
            </div>
            <div className='space-y-2'>
              <Label htmlFor={`tests.${index}.task_description`}>Task Description</Label>
              <Textarea
                id={`tests.${index}.task_description`}
                placeholder='Optional description for the task being tested'
                rows={3}
                {...register(`tests.${index}.task_description`)}
              />
            </div>
          </div>
        );

      case "custom":
        return (
          <div className='space-y-4'>
            <div className='space-y-2'>
              <Label htmlFor={`tests.${index}.dataset`}>Dataset</Label>
              <FilePathAutocompleteInput
                id={`tests.${index}.dataset`}
                fileExtension='.csv'
                datalistId={`test-dataset-${index}`}
                placeholder='Enter dataset name or path'
                {...register(`tests.${index}.dataset`, {
                  required: "Dataset is required for custom tests"
                })}
              />
              {testErrors?.dataset && (
                <p className='text-red-500 text-sm'>{String(testErrors.dataset.message || "")}</p>
              )}
            </div>
            <div className='space-y-2'>
              <Label htmlFor={`tests.${index}.workflow_variable_name`}>
                Workflow Variable Name
              </Label>
              <Input
                id={`tests.${index}.workflow_variable_name`}
                placeholder='Optional variable name to use in workflow'
                {...register(`tests.${index}.workflow_variable_name`)}
              />
            </div>
            <div className='flex items-center space-x-2'>
              <Controller
                name={`tests.${index}.is_context_id`}
                control={control}
                render={({ field: { value, onChange } }) => (
                  <input
                    type='checkbox'
                    id={`tests.${index}.is_context_id`}
                    checked={Boolean(value)}
                    onChange={onChange}
                    className='rounded'
                  />
                )}
              />
              <Label htmlFor={`tests.${index}.is_context_id`}>Is Context ID</Label>
            </div>
          </div>
        );

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
                <span className='font-medium text-sm'>Test {index + 1}</span>
                {testType && (
                  <span className='rounded-md bg-muted px-2 py-1 text-muted-foreground text-xs'>
                    {getTestTypeLabel(testType)}
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
            <div className='grid grid-cols-2 gap-4'>
              <div className='space-y-2'>
                <Label htmlFor={`tests.${index}.type`}>Test Type</Label>
                <Controller
                  name={`tests.${index}.type`}
                  control={control}
                  rules={{ required: "Test type is required" }}
                  render={({ field }) => (
                    <Select onValueChange={field.onChange} defaultValue={field.value}>
                      <SelectTrigger>
                        <SelectValue placeholder='Select test type' />
                      </SelectTrigger>
                      <SelectContent>
                        {TEST_TYPES.map((type) => (
                          <SelectItem key={type.value} value={type.value}>
                            {type.label}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  )}
                />
                {testErrors?.type && (
                  <p className='text-red-500 text-sm'>{String(testErrors.type.message || "")}</p>
                )}
              </div>
              <div className='space-y-2'>
                <Label htmlFor={`tests.${index}.concurrency`}>Concurrency</Label>
                <Input
                  id={`tests.${index}.concurrency`}
                  type='number'
                  min='1'
                  defaultValue={10}
                  {...register(`tests.${index}.concurrency`, {
                    valueAsNumber: true
                  })}
                />
              </div>
            </div>

            <div className='space-y-2'>
              <Label htmlFor={`tests.${index}.task_ref`}>Task Reference</Label>
              <Input
                id={`tests.${index}.task_ref`}
                placeholder='Optional reference to specific task'
                {...register(`tests.${index}.task_ref`)}
              />
            </div>

            {renderTestSpecificFields()}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
