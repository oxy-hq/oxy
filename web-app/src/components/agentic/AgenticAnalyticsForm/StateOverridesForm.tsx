import { ChevronDown, ChevronRight, X } from "lucide-react";
import { useState } from "react";
import { Controller, useFormContext } from "react-hook-form";
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
import { STATE_NAMES, THINKING_OPTIONS } from "./constants";
import type { AgenticFormData } from "./index";

type StateName = (typeof STATE_NAMES)[number];

interface StateItemProps {
  name: StateName;
}

const StateItem: React.FC<StateItemProps> = ({ name }) => {
  const [isOpen, setIsOpen] = useState(false);
  const { register, control } = useFormContext<AgenticFormData>();

  return (
    <div className='rounded-lg border p-3'>
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className='flex w-full items-center gap-2'>
          {isOpen ? (
            <ChevronDown className='h-4 w-4 text-muted-foreground' />
          ) : (
            <ChevronRight className='h-4 w-4 text-muted-foreground' />
          )}
          <span className='font-medium text-sm capitalize'>{name}</span>
        </CollapsibleTrigger>
        <CollapsibleContent className='mt-3 space-y-3'>
          {/* instructions */}
          <div className='space-y-2'>
            <Label htmlFor={`states.${name}.instructions`}>Instructions</Label>
            <Textarea
              id={`states.${name}.instructions`}
              placeholder='Additional instructions for this state only'
              rows={3}
              {...register(`states.${name}.instructions` as never)}
            />
          </div>

          {/* model — auto-suggest via free-text */}
          <div className='space-y-2'>
            <Label htmlFor={`states.${name}.model`}>Model Override</Label>
            <Input
              id={`states.${name}.model`}
              placeholder='e.g., claude-haiku-4-5 (inherits global if blank)'
              {...register(`states.${name}.model` as never)}
            />
            <p className='text-muted-foreground text-sm'>
              Only the model ID is replaced; vendor, API key, and base URL are inherited from the
              global LLM config.
            </p>
          </div>

          {/* max_retries */}
          <div className='space-y-2'>
            <Label htmlFor={`states.${name}.max_retries`}>Max Retries</Label>
            <Input
              id={`states.${name}.max_retries`}
              type='number'
              placeholder='e.g., 10'
              {...register(`states.${name}.max_retries` as never, { valueAsNumber: true })}
            />
          </div>

          {/* thinking override — auto-suggest with clear */}
          <div className='space-y-2'>
            <Label>Thinking Override</Label>
            <Controller
              name={`states.${name}.thinking` as never}
              control={control}
              render={({ field }) => (
                <div className='flex items-center gap-1'>
                  <Select
                    onValueChange={field.onChange}
                    value={(field.value as string | undefined) ?? ""}
                  >
                    <SelectTrigger className='flex-1'>
                      <SelectValue placeholder='Inherit from global config' />
                    </SelectTrigger>
                    <SelectContent>
                      {THINKING_OPTIONS.map((opt) => (
                        <SelectItem className='cursor-pointer' key={opt.value} value={opt.value}>
                          {opt.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  {field.value && (
                    <Button
                      type='button'
                      variant='ghost'
                      size='sm'
                      className='px-2'
                      onClick={() => field.onChange(undefined)}
                    >
                      <X className='h-4 w-4' />
                    </Button>
                  )}
                </div>
              )}
            />
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};

export const StateOverridesForm: React.FC = () => {
  return (
    <div className='space-y-4'>
      <div>
        <CardTitle>State Overrides</CardTitle>
        <p className='mt-1 text-muted-foreground text-sm'>
          Per-state configuration overrides. Takes precedence over global LLM settings.
        </p>
      </div>
      <div className='space-y-2'>
        {STATE_NAMES.map((name) => (
          <StateItem key={name} name={name} />
        ))}
      </div>
    </div>
  );
};
