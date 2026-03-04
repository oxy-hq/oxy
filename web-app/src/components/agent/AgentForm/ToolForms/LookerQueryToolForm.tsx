import type React from "react";
import { useMemo } from "react";
import { Controller, useFormContext } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import useLookerIntegrations from "@/hooks/api/integrations/useLookerIntegrations";
import type { AgentFormData } from "../index";

interface LookerQueryToolFormProps {
  index: number;
}

export const LookerQueryToolForm: React.FC<LookerQueryToolFormProps> = ({ index }) => {
  const { control, watch, setValue } = useFormContext<AgentFormData>();
  const { data: integrations, isLoading } = useLookerIntegrations();

  const selectedIntegration = watch(`tools.${index}.integration`) as string | undefined;
  const selectedModel = watch(`tools.${index}.model`) as string | undefined;

  const selectedIntegrationData = useMemo(
    () => integrations?.find((i) => i.name === selectedIntegration),
    [integrations, selectedIntegration]
  );

  const models = useMemo(() => {
    if (!selectedIntegrationData) return [];
    return [...new Set(selectedIntegrationData.explores.map((e) => e.model))];
  }, [selectedIntegrationData]);

  const explores = useMemo(() => {
    if (!selectedIntegrationData || !selectedModel) return [];
    return selectedIntegrationData.explores
      .filter((e) => e.model === selectedModel)
      .map((e) => e.name);
  }, [selectedIntegrationData, selectedModel]);

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label>Integration *</Label>
        <Controller
          name={`tools.${index}.integration`}
          control={control}
          rules={{ required: "Integration is required" }}
          render={({ field }) => (
            <Select
              disabled={isLoading}
              value={field.value as string}
              onValueChange={(val) => {
                field.onChange(val);
                setValue(`tools.${index}.model`, "");
                setValue(`tools.${index}.explore`, "");
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder={isLoading ? "Loading..." : "Select integration"} />
              </SelectTrigger>
              <SelectContent>
                {integrations?.map((i) => (
                  <SelectItem key={i.name} value={i.name}>
                    {i.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
      </div>

      <div className='space-y-2'>
        <Label>Model *</Label>
        <Controller
          name={`tools.${index}.model`}
          control={control}
          rules={{ required: "Model is required" }}
          render={({ field }) => (
            <Select
              disabled={!selectedIntegration || models.length === 0}
              value={field.value as string}
              onValueChange={(val) => {
                field.onChange(val);
                setValue(`tools.${index}.explore`, "");
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder='Select model' />
              </SelectTrigger>
              <SelectContent>
                {models.map((m) => (
                  <SelectItem key={m} value={m}>
                    {m}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
      </div>

      <div className='space-y-2'>
        <Label>Explore *</Label>
        <Controller
          name={`tools.${index}.explore`}
          control={control}
          rules={{ required: "Explore is required" }}
          render={({ field }) => (
            <Select
              disabled={!selectedModel || explores.length === 0}
              value={field.value as string}
              onValueChange={field.onChange}
            >
              <SelectTrigger>
                <SelectValue placeholder='Select explore' />
              </SelectTrigger>
              <SelectContent>
                {explores.map((e) => (
                  <SelectItem key={e} value={e}>
                    {e}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
      </div>
    </div>
  );
};
