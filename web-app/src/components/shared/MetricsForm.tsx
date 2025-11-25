/* eslint-disable @typescript-eslint/no-explicit-any */
import { useState } from "react";
import { useFormContext, Controller, FieldValues, Path } from "react-hook-form";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Textarea } from "@/components/ui/shadcn/textarea";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/shadcn/collapsible";
import { Plus, Trash2, ChevronDown, ChevronRight } from "lucide-react";

interface MetricsFormProps {
  testIndex: number;
}

const METRIC_TYPES = [
  { value: "similarity", label: "Similarity (LLM-based)" },
  { value: "context_recall", label: "Context Recall (Distance-based)" },
];

const DISTANCE_METHODS = [
  { value: "Levenshtein", label: "Levenshtein Distance" },
];

export function MetricsForm<T extends FieldValues>({
  testIndex,
}: MetricsFormProps) {
  const { register, control, watch, setValue, getValues } = useFormContext<T>();

  const [openMetricIndex, setOpenMetricIndex] = useState<number | null>(null);
  const [newlyAddedMetricIndex, setNewlyAddedMetricIndex] = useState<
    number | null
  >(null);

  const metrics =
    (watch(`tests.${testIndex}.metrics` as Path<T>) as unknown[]) || [];

  const addMetric = () => {
    const currentMetrics =
      (getValues(`tests.${testIndex}.metrics` as Path<T>) as unknown[]) || [];
    const newMetric = {
      type: "similarity",
    };
    const newIndex = currentMetrics.length;
    setValue(
      `tests.${testIndex}.metrics` as Path<T>,
      [...currentMetrics, newMetric] as any,
    );
    setNewlyAddedMetricIndex(newIndex);
    setOpenMetricIndex(newIndex);
  };

  const removeMetric = (metricIndex: number) => {
    const currentMetrics =
      (getValues(`tests.${testIndex}.metrics` as Path<T>) as unknown[]) || [];
    const updatedMetrics = currentMetrics.filter(
      (_, idx) => idx !== metricIndex,
    );
    setValue(`tests.${testIndex}.metrics` as Path<T>, updatedMetrics as any);
    if (openMetricIndex === metricIndex) {
      setOpenMetricIndex(null);
    }
  };

  const getMetricTypeLabel = (type: string | undefined) => {
    const metricTypeObj = METRIC_TYPES.find((t) => t.value === type);
    return metricTypeObj?.label || type || "Unknown";
  };

  const renderMetricFields = (
    metricIndex: number,
    metricType: string | undefined,
  ) => {
    const fieldPrefix = `tests.${testIndex}.metrics.${metricIndex}`;

    switch (metricType) {
      case "similarity":
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor={`${fieldPrefix}.model_ref`}>
                Model Reference
              </Label>
              <Input
                id={`${fieldPrefix}.model_ref`}
                placeholder="Optional model reference (uses default if not specified)"
                {...register(`${fieldPrefix}.model_ref` as Path<T>)}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor={`${fieldPrefix}.prompt`}>Evaluation Prompt</Label>
              <Textarea
                id={`${fieldPrefix}.prompt`}
                placeholder="Prompt for LLM evaluation"
                rows={8}
                className="font-mono text-sm"
                {...register(`${fieldPrefix}.prompt` as Path<T>, {
                  required: "Prompt is required for similarity metrics",
                })}
              />
            </div>
            <div className="space-y-2">
              <Label>Scoring Configuration</Label>
              <p className="text-sm text-muted-foreground mb-2">
                Map evaluation responses to numeric scores (0.0 to 1.0)
              </p>
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-1">
                  <Label htmlFor={`${fieldPrefix}.scores.A`}>
                    Score for "A"
                  </Label>
                  <Input
                    id={`${fieldPrefix}.scores.A`}
                    type="number"
                    min="0"
                    max="1"
                    step="0.1"
                    defaultValue="1.0"
                    {...register(`${fieldPrefix}.scores.A` as Path<T>, {
                      valueAsNumber: true,
                      required: "Score A is required",
                      min: {
                        value: 0,
                        message: "Score must be between 0 and 1",
                      },
                      max: {
                        value: 1,
                        message: "Score must be between 0 and 1",
                      },
                    })}
                  />
                </div>
                <div className="space-y-1">
                  <Label htmlFor={`${fieldPrefix}.scores.B`}>
                    Score for "B"
                  </Label>
                  <Input
                    id={`${fieldPrefix}.scores.B`}
                    type="number"
                    min="0"
                    max="1"
                    step="0.1"
                    defaultValue="0.0"
                    {...register(`${fieldPrefix}.scores.B` as Path<T>, {
                      valueAsNumber: true,
                      required: "Score B is required",
                      min: {
                        value: 0,
                        message: "Score must be between 0 and 1",
                      },
                      max: {
                        value: 1,
                        message: "Score must be between 0 and 1",
                      },
                    })}
                  />
                </div>
              </div>
            </div>
          </div>
        );

      case "context_recall":
        return (
          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor={`${fieldPrefix}.distance.distance`}>
                Distance Method
              </Label>
              <Controller
                name={`${fieldPrefix}.distance.distance` as Path<T>}
                control={control}
                defaultValue={"Levenshtein" as any}
                render={({ field }) => (
                  <Select
                    onValueChange={field.onChange}
                    defaultValue={field.value as string}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select distance method" />
                    </SelectTrigger>
                    <SelectContent>
                      {DISTANCE_METHODS.map((method) => (
                        <SelectItem key={method.value} value={method.value}>
                          {method.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor={`${fieldPrefix}.threshold`}>Threshold</Label>
              <Input
                id={`${fieldPrefix}.threshold`}
                type="number"
                min="0"
                max="1"
                step="0.1"
                defaultValue="0.5"
                {...register(`${fieldPrefix}.threshold` as Path<T>, {
                  valueAsNumber: true,
                  required: "Threshold is required",
                  min: {
                    value: 0,
                    message: "Threshold must be between 0 and 1",
                  },
                  max: {
                    value: 1,
                    message: "Threshold must be between 0 and 1",
                  },
                })}
              />
              <p className="text-sm text-muted-foreground">
                Similarity threshold (0.0 to 1.0). Higher values require more
                similarity.
              </p>
            </div>
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div className="space-y-4 border-t pt-4">
      <div className="flex items-center justify-between">
        <div>
          <Label className="text-base font-medium">Metrics Configuration</Label>
          <p className="text-sm text-muted-foreground mt-1">
            Configure evaluation metrics for this test. Default similarity
            metrics will be used if none are specified.
          </p>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={addMetric}
          className="flex items-center gap-2"
        >
          <Plus className="h-4 w-4" />
          Add Metric
        </Button>
      </div>

      {metrics.length === 0 && (
        <div className="text-center py-6 text-muted-foreground border-2 border-dashed rounded-lg">
          <p>No custom metrics configured.</p>
          <p className="text-sm">Default similarity metrics will be used.</p>
        </div>
      )}

      {metrics.map((_metric, metricIndex) => {
        const metricType = watch(
          `tests.${testIndex}.metrics.${metricIndex}.type` as Path<T>,
        ) as string | undefined;
        const isOpen =
          openMetricIndex === metricIndex ||
          newlyAddedMetricIndex === metricIndex;

        return (
          <div key={metricIndex} className="rounded-lg border bg-card p-3">
            <Collapsible
              open={isOpen}
              onOpenChange={(open) => {
                setOpenMetricIndex(open ? metricIndex : null);
                if (newlyAddedMetricIndex === metricIndex) {
                  setNewlyAddedMetricIndex(null);
                }
              }}
            >
              <CollapsibleTrigger className="rounded-lg transition-colors w-full">
                <div className="flex items-center justify-between transition-colors">
                  {isOpen ? (
                    <ChevronDown className="h-5 w-5 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-5 w-5 text-muted-foreground" />
                  )}
                  <div className="flex items-center gap-3 flex-1">
                    <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary font-semibold text-sm">
                      {metricIndex + 1}
                    </span>
                    <div className="flex items-center gap-2 flex-1">
                      <span className="font-medium text-sm">
                        Metric {metricIndex + 1}
                      </span>
                      {metricType && (
                        <span className="text-xs px-2 py-1 rounded-md bg-muted text-muted-foreground">
                          {getMetricTypeLabel(metricType)}
                        </span>
                      )}
                    </div>
                  </div>
                  <Button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      removeMetric(metricIndex);
                    }}
                    variant="ghost"
                    size="sm"
                  >
                    <Trash2 className="w-4 h-4" />
                  </Button>
                </div>
              </CollapsibleTrigger>

              <CollapsibleContent className="space-y-4 mt-4">
                <div className="space-y-4">
                  <div className="space-y-2">
                    <Label
                      htmlFor={`tests.${testIndex}.metrics.${metricIndex}.type`}
                    >
                      Metric Type
                    </Label>
                    <Controller
                      name={
                        `tests.${testIndex}.metrics.${metricIndex}.type` as Path<T>
                      }
                      control={control}
                      rules={{ required: "Metric type is required" }}
                      render={({ field }) => (
                        <Select
                          onValueChange={field.onChange}
                          defaultValue={field.value as string}
                        >
                          <SelectTrigger>
                            <SelectValue placeholder="Select metric type" />
                          </SelectTrigger>
                          <SelectContent>
                            {METRIC_TYPES.map((type) => (
                              <SelectItem key={type.value} value={type.value}>
                                {type.label}
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                      )}
                    />
                  </div>

                  {renderMetricFields(metricIndex, metricType)}
                </div>
              </CollapsibleContent>
            </Collapsible>
          </div>
        );
      })}
    </div>
  );
}
