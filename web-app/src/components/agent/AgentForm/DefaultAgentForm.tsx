import { useFormContext } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { ContextForm } from "./ContextForm";
import type { AgentFormData } from "./index";
import { ToolsForm } from "./ToolsForm";

export const DefaultAgentForm: React.FC = () => {
  const {
    register,
    formState: { errors }
  } = useFormContext<AgentFormData>();

  return (
    <>
      <div className='space-y-2'>
        <Label htmlFor='system_instructions'>System Instructions *</Label>
        <Textarea
          id='system_instructions'
          placeholder='Enter the system instructions for the agent...'
          {...register("system_instructions", {
            required: "System instructions are required"
          })}
          rows={6}
        />
        {errors.system_instructions && (
          <p className='text-red-500 text-sm'>{errors.system_instructions.message}</p>
        )}
      </div>

      <div className='grid grid-cols-2 gap-4'>
        <div className='space-y-2'>
          <Label htmlFor='max_tool_calls'>Max Tool Calls</Label>
          <Input
            id='max_tool_calls'
            type='number'
            min='1'
            {...register("max_tool_calls", {
              valueAsNumber: true
            })}
          />
        </div>

        <div className='space-y-2'>
          <Label htmlFor='max_tool_concurrency'>Max Tool Concurrency</Label>
          <Input
            id='max_tool_concurrency'
            type='number'
            min='1'
            {...register("max_tool_concurrency", {
              valueAsNumber: true
            })}
          />
        </div>
      </div>

      <ContextForm />

      <ToolsForm />
    </>
  );
};
