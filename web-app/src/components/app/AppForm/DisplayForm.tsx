import React, { useState } from "react";
import { useFormContext, Controller } from "react-hook-form";
import { Label } from "@/components/ui/shadcn/label";
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
import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import { AppFormData } from "./index";
import {
  MarkdownDisplayFields,
  LineChartDisplayFields,
  BarChartDisplayFields,
  PieChartDisplayFields,
  TableDisplayFields,
} from "./DisplayFields";

interface DisplayFormProps {
  index: number;
  onRemove: () => void;
}

const DISPLAY_TYPES = [
  { value: "markdown", label: "Markdown" },
  { value: "line_chart", label: "Line Chart" },
  { value: "pie_chart", label: "Pie Chart" },
  { value: "bar_chart", label: "Bar Chart" },
  { value: "table", label: "Table" },
];

export const DisplayForm: React.FC<DisplayFormProps> = ({
  index,
  onRemove,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const {
    control,
    watch,
    setValue,
    formState: { errors },
  } = useFormContext<AppFormData>();

  const displayType = watch(`display.${index}.type`);
  const displayErrors = errors.display?.[index];

  const getDisplayTypeLabel = (type: string) => {
    return DISPLAY_TYPES.find((t) => t.value === type)?.label || type;
  };

  const renderDisplaySpecificFields = () => {
    switch (displayType) {
      case "markdown":
        return <MarkdownDisplayFields index={index} />;

      case "line_chart":
        return <LineChartDisplayFields index={index} />;

      case "bar_chart":
        return <BarChartDisplayFields index={index} />;

      case "pie_chart":
        return <PieChartDisplayFields index={index} />;

      case "table":
        return <TableDisplayFields index={index} />;

      default:
        return null;
    }
  };

  const onTypeChange = (value: string) => {
    if (value !== displayType) {
      setValue(`display.${index}`, {
        type: value,
      });
    }
  };

  return (
    <div className="rounded-lg border bg-card p-3">
      <Collapsible open={isOpen} onOpenChange={setIsOpen}>
        <CollapsibleTrigger className="rounded-lg transition-colors w-full">
          <div className="flex items-center justify-between transition-colors">
            {isOpen ? (
              <ChevronDown className="h-5 w-5 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-5 w-5 text-muted-foreground" />
            )}
            <div className="flex items-center gap-3 flex-1">
              <span className="flex items-center justify-center w-8 h-8 rounded-full bg-primary/10 text-primary font-semibold text-sm">
                {index + 1}
              </span>
              <div className="flex items-center gap-2 flex-1">
                <span className="font-medium text-sm">Display {index + 1}</span>
                {displayType && (
                  <span className="text-xs px-2 py-1 rounded-md bg-muted text-muted-foreground">
                    {getDisplayTypeLabel(displayType)}
                  </span>
                )}
              </div>
            </div>
            <Button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onRemove();
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
              <Label htmlFor={`display.${index}.type`}>Type *</Label>
              <Controller
                name={`display.${index}.type`}
                control={control}
                rules={{ required: "Display type is required" }}
                render={({ field }) => (
                  <Select
                    onValueChange={(value) => {
                      onTypeChange(value);
                      field.onChange(value);
                    }}
                    defaultValue={field.value}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select display type" />
                    </SelectTrigger>
                    <SelectContent>
                      {DISPLAY_TYPES.map((type) => (
                        <SelectItem key={type.value} value={type.value}>
                          {type.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                )}
              />
              {displayErrors?.type && (
                <p className="text-sm text-red-500">
                  {String(displayErrors.type.message || "")}
                </p>
              )}
            </div>

            {renderDisplaySpecificFields()}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
