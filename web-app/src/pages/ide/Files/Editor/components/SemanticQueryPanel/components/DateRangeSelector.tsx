import { Calendar as CalendarIcon, Clock2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Calendar } from "@/components/ui/shadcn/calendar";
import { FieldDescription } from "@/components/ui/shadcn/field";
import { Input } from "@/components/ui/shadcn/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/shadcn/input-group";
import { Label } from "@/components/ui/shadcn/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import type { DateRangeValue } from "@/services/api/semantic";

interface DateRangeSelectorProps {
  className?: string;
  from?: DateRangeValue;
  to?: DateRangeValue;
  onChange: (updates: { relative?: string; from?: DateRangeValue; to?: DateRangeValue }) => void;
}

const RELATIVE_PRESETS = [
  { label: "Today", value: "today" },
  { label: "Yesterday", value: "yesterday" },
  { label: "1 Week Ago", value: "1 week ago" },
  { label: "1 Month Ago", value: "1 month ago" },
  { label: "1 Year Ago", value: "1 year ago" },
  { label: "Custom", value: "custom" }
];

function parseToDate(value?: DateRangeValue): Date | undefined {
  if (!value) return undefined;
  if (value instanceof Date) return value;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function formatTimeValue(value?: DateRangeValue): string {
  const date = parseToDate(value);
  if (!date) return "12:00:00";
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}:${String(date.getSeconds()).padStart(2, "0")}`;
}

const formatDisplayValue = (value?: DateRangeValue): string => {
  if (!value) return "Select...";
  if (typeof value === "string") {
    const date = new Date(value);
    if (!Number.isNaN(date.getTime())) {
      return new Intl.DateTimeFormat("en-US", {
        dateStyle: "medium",
        timeStyle: "short"
      }).format(date);
    }
    return value;
  }
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(value);
};

interface DatePickerPopoverProps {
  className?: string;
  value?: DateRangeValue;
  otherValue?: DateRangeValue;
  field: "from" | "to";
  onChange: DateRangeSelectorProps["onChange"];
}

const DatePickerPopover = ({
  className,
  value,
  otherValue,
  field,
  onChange
}: DatePickerPopoverProps) => {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<string>("calendar");
  const [relativePreset, setRelativePreset] = useState<string>("custom");
  const [customRelative, setCustomRelative] = useState<string>("");
  const [error, setError] = useState<string>("");

  const selectedDate = parseToDate(value);

  const handleDaySelect = (day: Date | undefined) => {
    if (!day) return;
    const existing = parseToDate(value);
    if (existing) {
      day.setHours(existing.getHours(), existing.getMinutes());
    }
    const iso = day.toISOString();
    onChange({
      relative: undefined,
      from: field === "from" ? iso : otherValue,
      to: field === "to" ? iso : otherValue
    });
  };

  const handleTimeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const [hours, minutes, seconds] = e.target.value.split(":").map(Number);
    const base = parseToDate(value) ?? new Date();
    base.setHours(hours ?? 0, minutes ?? 0, seconds ?? 0);
    const iso = base.toISOString();
    onChange({
      relative: undefined,
      from: field === "from" ? iso : otherValue,
      to: field === "to" ? iso : otherValue
    });
  };

  const handlePresetChange = (preset: string) => {
    setRelativePreset(preset);
    if (preset !== "custom") {
      onChange({
        relative: undefined,
        from: field === "from" ? preset : otherValue,
        to: field === "to" ? preset : otherValue
      });
      setOpen(false);
    }
  };

  const handleCustomApply = () => {
    const trimmed = customRelative.trim();
    if (!trimmed) {
      setError("Please enter a value");
      return;
    }
    setError("");
    onChange({
      relative: undefined,
      from: field === "from" ? trimmed : otherValue,
      to: field === "to" ? trimmed : otherValue
    });
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild className={className}>
        <Button variant='outline' className='flex-1 bg-input/30 font-normal'>
          <CalendarIcon />
          <span className='flex-1 truncate text-left'>{formatDisplayValue(value)}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-auto p-0' align='start'>
        <Tabs className='max-w-[300px] space-y-2 p-3' value={tab} onValueChange={setTab}>
          <TabsList className='grid w-full grid-cols-2'>
            <TabsTrigger value='calendar'>Calendar</TabsTrigger>
            <TabsTrigger value='relative'>Relative</TabsTrigger>
          </TabsList>
          <TabsContent value='calendar'>
            <div className='space-y-2'>
              <Calendar
                mode='single'
                className='p-0'
                selected={selectedDate}
                onSelect={handleDaySelect}
                defaultMonth={selectedDate}
              />
              <InputGroup>
                <InputGroupInput
                  type='time'
                  step='1'
                  value={formatTimeValue(value)}
                  onChange={handleTimeChange}
                  className='appearance-none [&::-webkit-calendar-picker-indicator]:hidden [&::-webkit-calendar-picker-indicator]:appearance-none'
                />
                <InputGroupAddon>
                  <Clock2Icon className='text-muted-foreground' />
                </InputGroupAddon>
              </InputGroup>
            </div>
          </TabsContent>
          <TabsContent value='relative' className='space-y-2'>
            <Label>Relative time</Label>
            <Select value={relativePreset} onValueChange={handlePresetChange}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RELATIVE_PRESETS.map((preset) => (
                  <SelectItem className='cursor-pointer' key={preset.value} value={preset.value}>
                    {preset.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {relativePreset === "custom" && (
              <div className='space-y-2'>
                <div className='flex gap-2'>
                  <Input
                    type='text'
                    placeholder='e.g., 7 days ago'
                    value={customRelative}
                    onChange={(e) => {
                      setCustomRelative(e.target.value);
                      if (error) setError("");
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        handleCustomApply();
                      }
                    }}
                  />
                  <Button type='button' size='sm' onClick={handleCustomApply}>
                    Apply
                  </Button>
                </div>
                {error && <p className='text-destructive'>{error}</p>}
                <FieldDescription>
                  Supports chrono-english syntax, e.g. "7 days ago", "3 months ago".
                </FieldDescription>
              </div>
            )}
          </TabsContent>
        </Tabs>
      </PopoverContent>
    </Popover>
  );
};

const DateRangeSelector = ({ from, to, onChange, className }: DateRangeSelectorProps) => {
  return (
    <div className='flex flex-1 items-center gap-2'>
      <DatePickerPopover
        className={className}
        value={from}
        otherValue={to}
        field='from'
        onChange={onChange}
      />
      <span className='text-muted-foreground text-xs'>→</span>
      <DatePickerPopover
        className={className}
        value={to}
        otherValue={from}
        field='to'
        onChange={onChange}
      />
    </div>
  );
};

export default DateRangeSelector;
