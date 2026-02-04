import { ChevronDown, ChevronRight, Trash2 } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Controller, useFormContext } from "react-hook-form";
import { Button } from "@/components/ui/shadcn/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger
} from "@/components/ui/shadcn/collapsible";
import { Label } from "@/components/ui/shadcn/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import {
  BarChartDisplayFields,
  LineChartDisplayFields,
  MarkdownDisplayFields,
  PieChartDisplayFields,
  TableDisplayFields
} from "./DisplayFields";
import type { AppFormData } from "./index";

interface DisplayFormProps {
  index: number;
  onRemove: () => void;
}

const DISPLAY_TYPES = [
  { value: "markdown", label: "Markdown" },
  { value: "line_chart", label: "Line Chart" },
  { value: "pie_chart", label: "Pie Chart" },
  { value: "bar_chart", label: "Bar Chart" },
  { value: "table", label: "Table" }
];

export const DisplayForm: React.FC<DisplayFormProps> = ({ index, onRemove }) => {
  const [isOpen, setIsOpen] = useState(false);
  const {
    control,
    watch,
    setValue,
    formState: { errors }
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
        type: value
      });
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
                <span className='font-medium text-sm'>Display {index + 1}</span>
                {displayType && (
                  <span className='rounded-md bg-muted px-2 py-1 text-muted-foreground text-xs'>
                    {getDisplayTypeLabel(displayType)}
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
            <div className='space-y-2'>
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
                      <SelectValue placeholder='Select display type' />
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
                <p className='text-red-500 text-sm'>{String(displayErrors.type.message || "")}</p>
              )}
            </div>

            {renderDisplaySpecificFields()}
          </div>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
};
