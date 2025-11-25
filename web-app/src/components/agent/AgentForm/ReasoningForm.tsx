import React from "react";
import { useFormContext, Controller } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import { CardTitle } from "@/components/ui/shadcn/card";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Plus, Trash2 } from "lucide-react";
import { AgentFormData } from "./index";

const REASONING_EFFORTS = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
] as const;

export const ReasoningForm: React.FC = () => {
  const { control, watch, setValue } = useFormContext<AgentFormData>();

  const reasoning = watch("reasoning");
  const [showReasoningEffort, setShowReasoningEffort] = React.useState(
    !!reasoning?.effort,
  );

  return (
    <div className="space-y-4">
      <CardTitle>Reasoning (Optional)</CardTitle>

      {!showReasoningEffort ? (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setShowReasoningEffort(true)}
        >
          <Plus className="h-4 w-4" />
          Add Reasoning Effort
        </Button>
      ) : (
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label htmlFor="reasoning.effort">Reasoning Effort</Label>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => {
                setValue("reasoning.effort", undefined);
                setShowReasoningEffort(false);
              }}
            >
              <Trash2 className="h-4 w-4" />
              Remove
            </Button>
          </div>
          <Controller
            name="reasoning.effort"
            control={control}
            render={({ field }) => (
              <Select
                onValueChange={field.onChange}
                value={field.value || (reasoning?.effort as string)}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Select reasoning effort level" />
                </SelectTrigger>
                <SelectContent>
                  {REASONING_EFFORTS.map((effort) => (
                    <SelectItem key={effort.value} value={effort.value}>
                      {effort.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          />
          <p className="text-sm text-muted-foreground">
            Control how much reasoning the agent applies to tasks
          </p>
        </div>
      )}
    </div>
  );
};
