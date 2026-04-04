import { ChevronDown, ChevronRight, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Controller, useFieldArray, useFormContext } from "react-hook-form";
import { Checkbox } from "@/components/ui/checkbox";
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
import { SQL_DIALECT_OPTIONS, VALIDATION_RULE_NAMES } from "./constants";
import type { AgenticFormData, ValidationRuleData } from "./index";

// Rules that have extra params fields in the form
const RULES_WITH_DIALECT = new Set(["sql_syntax"]);
const RULES_WITH_OUTLIER_PARAMS = new Set(["outlier_detection"]);

interface RuleItemProps {
  stage: "specified" | "solvable" | "solved";
  index: number;
  onRemove: () => void;
}

const RuleItem: React.FC<RuleItemProps> = ({ stage, index, onRemove }) => {
  const [isOpen, setIsOpen] = useState(true);
  const { register, control, watch } = useFormContext<AgenticFormData>();
  const ruleName = watch(`validation.rules.${stage}.${index}.name` as never) as unknown as
    | string
    | undefined;

  return (
    <div className='rounded-lg border p-3'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='flex w-full items-center justify-between'>
          <div className='flex items-center gap-2'>
            {isOpen ? (
              <ChevronDown className='h-4 w-4 text-muted-foreground' />
            ) : (
              <ChevronRight className='h-4 w-4 text-muted-foreground' />
            )}
            <span className='font-medium text-sm'>
              {VALIDATION_RULE_NAMES.find((r) => r.value === ruleName)?.label ??
                ruleName ??
                `Rule ${index + 1}`}
            </span>
          </div>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={(e) => {
              e.stopPropagation();
              onRemove();
            }}
          >
            <Trash2 className='h-4 w-4' />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent className='mt-3 space-y-3'>
          {/* name — auto-suggest */}
          <div className='space-y-2'>
            <Label>
              Rule Name <span className='text-destructive'>*</span>
            </Label>
            <Controller
              name={`validation.rules.${stage}.${index}.name` as never}
              control={control}
              render={({ field }) => (
                <Select
                  onValueChange={field.onChange}
                  value={(field.value as string | undefined) ?? ""}
                >
                  <SelectTrigger>
                    <SelectValue placeholder='Select rule' />
                  </SelectTrigger>
                  <SelectContent>
                    {VALIDATION_RULE_NAMES.map((r) => (
                      <SelectItem className='cursor-pointer' key={r.value} value={r.value}>
                        {r.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              )}
            />
          </div>

          {/* enabled */}
          <div className='flex items-center gap-2'>
            <Controller
              name={`validation.rules.${stage}.${index}.enabled` as never}
              control={control}
              render={({ field }) => (
                <Checkbox
                  id={`validation.rules.${stage}.${index}.enabled`}
                  checked={(field.value as boolean | undefined) ?? true}
                  onCheckedChange={field.onChange}
                />
              )}
            />
            <Label htmlFor={`validation.rules.${stage}.${index}.enabled`}>Enabled</Label>
          </div>

          {/* sql_syntax: dialect */}
          {RULES_WITH_DIALECT.has(ruleName ?? "") && (
            <div className='space-y-2'>
              <Label>SQL Dialect</Label>
              <Controller
                name={`validation.rules.${stage}.${index}.dialect` as never}
                control={control}
                render={({ field }) => (
                  <Select
                    onValueChange={field.onChange}
                    value={(field.value as string | undefined) ?? ""}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder='Default: generic' />
                    </SelectTrigger>
                    <SelectContent>
                      {SQL_DIALECT_OPTIONS.map((opt) => (
                        <SelectItem className='cursor-pointer' key={opt.value} value={opt.value}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
            </div>
          )}

          {/* outlier_detection params */}
          {RULES_WITH_OUTLIER_PARAMS.has(ruleName ?? "") && (
            <>
              <div className='space-y-2'>
                <Label htmlFor={`validation.rules.${stage}.${index}.threshold_sigma`}>
                  Threshold Sigma
                </Label>
                <Input
                  id={`validation.rules.${stage}.${index}.threshold_sigma`}
                  type='number'
                  step='0.1'
                  placeholder='Default: 5.0'
                  {...register(`validation.rules.${stage}.${index}.threshold_sigma` as never, {
                    valueAsNumber: true
                  })}
                />
              </div>
              <div className='space-y-2'>
                <Label htmlFor={`validation.rules.${stage}.${index}.min_rows`}>Min Rows</Label>
                <Input
                  id={`validation.rules.${stage}.${index}.min_rows`}
                  type='number'
                  placeholder='Default: 4'
                  {...register(`validation.rules.${stage}.${index}.min_rows` as never, {
                    valueAsNumber: true
                  })}
                />
              </div>
            </>
          )}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

interface StageGroupProps {
  stage: "specified" | "solvable" | "solved";
  label: string;
  description: string;
}

const StageGroup: React.FC<StageGroupProps> = ({ stage, label, description }) => {
  const { control } = useFormContext<AgenticFormData>();
  const { fields, append, remove } = useFieldArray({
    control,
    name: `validation.rules.${stage}` as never
  });

  return (
    <div className='space-y-3'>
      <div className='flex items-center justify-between'>
        <div>
          <p className='font-medium text-sm'>{label}</p>
          <p className='text-muted-foreground text-xs'>{description}</p>
        </div>
        <Button
          type='button'
          variant='outline'
          size='sm'
          onClick={() => append({ name: "", enabled: true } as ValidationRuleData)}
        >
          <Plus />
          Add Rule
        </Button>
      </div>
      <div className='space-y-2'>
        {(fields as unknown[]).map((_, index) => (
          <RuleItem
            // biome-ignore lint/suspicious/noArrayIndexKey: stable for field arrays
            key={index}
            stage={stage}
            index={index}
            onRemove={() => remove(index)}
          />
        ))}
      </div>
    </div>
  );
};

export const ValidationForm: React.FC = () => {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className='space-y-4'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='flex w-full items-center gap-2'>
          {isOpen ? (
            <ChevronDown className='h-4 w-4 text-muted-foreground' />
          ) : (
            <ChevronRight className='h-4 w-4 text-muted-foreground' />
          )}
          <CardTitle>Validation</CardTitle>
        </CollapsibleTrigger>
        <CollapsibleContent className='mt-4 space-y-6'>
          <p className='text-muted-foreground text-sm'>
            Validation rules applied after each pipeline stage. All built-in rules are enabled by
            default.
          </p>
          <StageGroup
            stage='specified'
            label='After Specify'
            description='Run after the Specifying state'
          />
          <StageGroup
            stage='solvable'
            label='After Solve'
            description='Run after the Solving state'
          />
          <StageGroup
            stage='solved'
            label='After Execute'
            description='Run after the Executing state'
          />
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
