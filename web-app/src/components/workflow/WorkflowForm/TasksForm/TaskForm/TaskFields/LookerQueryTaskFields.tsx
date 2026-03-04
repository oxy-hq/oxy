import { Plus, X } from "lucide-react";
import type React from "react";
import { Controller, useFieldArray, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import useLookerIntegrations from "@/hooks/api/integrations/useLookerIntegrations";
import type { WorkflowFormData } from "../..";

interface LookerQueryTaskFieldsProps {
  index: number;
  basePath?: string;
}

export const LookerQueryTaskFields: React.FC<LookerQueryTaskFieldsProps> = ({
  index,
  basePath = "tasks"
}) => {
  const {
    register,
    control,
    watch,
    setValue,
    formState: { errors }
  } = useFormContext<WorkflowFormData>();

  const taskPath = `${basePath}.${index}`;
  // @ts-expect-error - Dynamic path for nested tasks
  const taskErrors = errors[basePath]?.[index];

  // @ts-expect-error - dynamic field path
  const selectedIntegration = watch(`${taskPath}.integration`) as string | undefined;
  // @ts-expect-error - dynamic field path
  const selectedModel = watch(`${taskPath}.model`) as string | undefined;
  // @ts-expect-error - dynamic field path
  const selectedExplore = watch(`${taskPath}.explore`) as string | undefined;

  const { data: lookerIntegrations = [] } = useLookerIntegrations();

  const currentIntegration = lookerIntegrations.find((i) => i.name === selectedIntegration);

  const availableModels = currentIntegration
    ? [...new Set(currentIntegration.explores.map((e) => e.model))]
    : [];

  const availableExplores = currentIntegration
    ? currentIntegration.explores.filter((e) => e.model === selectedModel)
    : [];

  const currentExplore = availableExplores.find((explore) => explore.name === selectedExplore);
  const availableFieldOptions = currentExplore?.fields ?? [];

  const {
    fields: fieldEntries,
    append: appendField,
    remove: removeField
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.fields`
  });

  const {
    fields: filterEntries,
    append: appendFilter,
    remove: removeFilter
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.filters`
  });

  const {
    fields: sortEntries,
    append: appendSort,
    remove: removeSort
  } = useFieldArray({
    control,
    // @ts-expect-error - dynamic field path
    name: `${taskPath}.sorts`
  });

  return (
    <div className='space-y-4'>
      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.integration`}>Integration</Label>
        <Controller
          control={control}
          // @ts-expect-error - dynamic field path
          name={`${taskPath}.integration`}
          rules={{ required: "Integration is required" }}
          render={({ field }) => (
            <Select
              value={(field.value as string) || ""}
              onValueChange={(value) => {
                field.onChange(value);
                // @ts-expect-error - dynamic field path
                setValue(`${taskPath}.model`, "");
                // @ts-expect-error - dynamic field path
                setValue(`${taskPath}.explore`, "");
              }}
            >
              <SelectTrigger>
                <SelectValue placeholder='Select integration' />
              </SelectTrigger>
              <SelectContent>
                {lookerIntegrations.map((integration) => (
                  <SelectItem key={integration.name} value={integration.name}>
                    {integration.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          )}
        />
        {taskErrors?.integration && (
          <p className='text-red-500 text-sm'>{taskErrors.integration.message}</p>
        )}
      </div>

      <div className='grid grid-cols-2 gap-4'>
        <div className='space-y-2'>
          <Label htmlFor={`${taskPath}.model`}>Model</Label>
          <Controller
            control={control}
            // @ts-expect-error - dynamic field path
            name={`${taskPath}.model`}
            rules={{ required: "Model is required" }}
            render={({ field }) => (
              <Select
                value={(field.value as string) || ""}
                disabled={!selectedIntegration}
                onValueChange={(value) => {
                  field.onChange(value);
                  // @ts-expect-error - dynamic field path
                  setValue(`${taskPath}.explore`, "");
                }}
              >
                <SelectTrigger>
                  <SelectValue
                    placeholder={selectedIntegration ? "Select model" : "Select integration first"}
                  />
                </SelectTrigger>
                <SelectContent>
                  {availableModels.map((model) => (
                    <SelectItem key={model} value={model}>
                      {model}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          />
          {taskErrors?.model && <p className='text-red-500 text-sm'>{taskErrors.model.message}</p>}
        </div>

        <div className='space-y-2'>
          <Label htmlFor={`${taskPath}.explore`}>Explore</Label>
          <Controller
            control={control}
            // @ts-expect-error - dynamic field path
            name={`${taskPath}.explore`}
            rules={{ required: "Explore is required" }}
            render={({ field }) => (
              <Select
                value={(field.value as string) || ""}
                disabled={!selectedModel}
                onValueChange={(value) => {
                  field.onChange(value);
                  // @ts-expect-error - dynamic field path
                  setValue(`${taskPath}.fields`, []);
                  // @ts-expect-error - dynamic field path
                  setValue(`${taskPath}.filters`, []);
                  // @ts-expect-error - dynamic field path
                  setValue(`${taskPath}.sorts`, []);
                }}
              >
                <SelectTrigger>
                  <SelectValue
                    placeholder={selectedModel ? "Select explore" : "Select model first"}
                  />
                </SelectTrigger>
                <SelectContent>
                  {availableExplores.map((explore) => (
                    <SelectItem key={explore.name} value={explore.name}>
                      {explore.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          />
          {taskErrors?.explore && (
            <p className='text-red-500 text-sm'>{taskErrors.explore.message}</p>
          )}
        </div>
      </div>

      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Fields</Label>
          <Button
            type='button'
            onClick={() => appendField("" as never)}
            variant='outline'
            size='sm'
          >
            <Plus className='mr-1 h-4 w-4' />
            Add Field
          </Button>
        </div>
        {fieldEntries.length > 0 && (
          <div className='space-y-2'>
            {fieldEntries.map((field, fieldIndex) => (
              <div key={field.id} className='flex items-start gap-2'>
                <div className='flex-1'>
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.fields.${fieldIndex}`}
                    render={({ field: selectedField }) => (
                      <Select
                        disabled={!selectedExplore || availableFieldOptions.length === 0}
                        value={(selectedField.value as string) || ""}
                        onValueChange={selectedField.onChange}
                      >
                        <SelectTrigger>
                          <SelectValue
                            placeholder={
                              selectedExplore
                                ? availableFieldOptions.length > 0
                                  ? "Select field"
                                  : "No synced fields available"
                                : "Select explore first"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent>
                          {availableFieldOptions.map((fieldName) => (
                            <SelectItem key={fieldName} value={fieldName}>
                              {fieldName}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  />
                </div>
                <Button
                  type='button'
                  onClick={() => removeField(fieldIndex)}
                  variant='ghost'
                  size='sm'
                >
                  <X className='h-4 w-4' />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className='text-muted-foreground text-sm'>
          Fields are populated from synced Looker metadata
        </p>
      </div>

      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Filters</Label>
          <Button
            type='button'
            onClick={() => appendFilter({ key: "", value: "" } as never)}
            variant='outline'
            size='sm'
          >
            <Plus className='mr-1 h-4 w-4' />
            Add Filter
          </Button>
        </div>
        {filterEntries.length > 0 && (
          <div className='space-y-2'>
            {filterEntries.map((filter, filterIndex) => (
              <div key={filter.id} className='flex items-start gap-2'>
                <div className='flex-1'>
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.filters.${filterIndex}.key`}
                    render={({ field: filterField }) => (
                      <Select
                        disabled={!selectedExplore || availableFieldOptions.length === 0}
                        value={(filterField.value as string) || ""}
                        onValueChange={filterField.onChange}
                      >
                        <SelectTrigger>
                          <SelectValue
                            placeholder={
                              selectedExplore
                                ? availableFieldOptions.length > 0
                                  ? "Select filter field"
                                  : "No synced fields available"
                                : "Select explore first"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent>
                          {availableFieldOptions.map((fieldName) => (
                            <SelectItem key={fieldName} value={fieldName}>
                              {fieldName}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  />
                </div>
                <div className='flex-1'>
                  <Input
                    placeholder='Filter expression'
                    // @ts-expect-error - dynamic field path
                    {...register(`${taskPath}.filters.${filterIndex}.value`)}
                  />
                </div>
                <Button
                  type='button'
                  onClick={() => removeFilter(filterIndex)}
                  variant='ghost'
                  size='sm'
                >
                  <X className='h-4 w-4' />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className='text-muted-foreground text-sm'>
          Filter conditions as field name to Looker filter expression mappings
        </p>
      </div>

      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.filter_expression`}>Filter Expression</Label>
        <Input
          id={`${taskPath}.filter_expression`}
          placeholder='Optional Looker filter expression for complex conditions'
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.filter_expression`)}
        />
      </div>

      <div className='space-y-2'>
        <div className='flex items-center justify-between'>
          <Label>Sort Fields</Label>
          <Button
            type='button'
            onClick={() => appendSort({ field: "", direction: "asc" } as never)}
            variant='outline'
            size='sm'
          >
            <Plus className='mr-1 h-4 w-4' />
            Add Sort
          </Button>
        </div>
        {sortEntries.length > 0 && (
          <div className='space-y-2'>
            {sortEntries.map((sort, sortIndex) => (
              <div key={sort.id} className='flex items-start gap-2'>
                <div className='flex-1'>
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.sorts.${sortIndex}.field`}
                    render={({ field: sortField }) => (
                      <Select
                        disabled={!selectedExplore || availableFieldOptions.length === 0}
                        value={(sortField.value as string) || ""}
                        onValueChange={sortField.onChange}
                      >
                        <SelectTrigger>
                          <SelectValue
                            placeholder={
                              selectedExplore
                                ? availableFieldOptions.length > 0
                                  ? "Select sort field"
                                  : "No synced fields available"
                                : "Select explore first"
                            }
                          />
                        </SelectTrigger>
                        <SelectContent>
                          {availableFieldOptions.map((fieldName) => (
                            <SelectItem key={fieldName} value={fieldName}>
                              {fieldName}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    )}
                  />
                </div>
                <div className='w-24'>
                  <Controller
                    control={control}
                    // @ts-expect-error - dynamic field path
                    name={`${taskPath}.sorts.${sortIndex}.direction`}
                    render={({ field: dirField }) => (
                      <Select
                        value={(dirField.value as string) || "asc"}
                        onValueChange={dirField.onChange}
                      >
                        <SelectTrigger>
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value='asc'>Asc</SelectItem>
                          <SelectItem value='desc'>Desc</SelectItem>
                        </SelectContent>
                      </Select>
                    )}
                  />
                </div>
                <Button
                  type='button'
                  onClick={() => removeSort(sortIndex)}
                  variant='ghost'
                  size='sm'
                >
                  <X className='h-4 w-4' />
                </Button>
              </div>
            ))}
          </div>
        )}
        <p className='text-muted-foreground text-sm'>
          Sort fields are populated from synced metadata with ascending/descending options
        </p>
      </div>

      <div className='space-y-2'>
        <Label htmlFor={`${taskPath}.limit`}>Limit</Label>
        <Input
          id={`${taskPath}.limit`}
          type='number'
          min='-1'
          placeholder='Optional row limit (-1 for unlimited)'
          // @ts-expect-error - dynamic field path
          {...register(`${taskPath}.limit`, {
            valueAsNumber: true
          })}
        />
      </div>
    </div>
  );
};
