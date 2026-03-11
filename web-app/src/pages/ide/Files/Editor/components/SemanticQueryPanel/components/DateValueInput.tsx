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

const RELATIVE_PRESETS = [
  { label: "Today", value: "today" },
  { label: "Yesterday", value: "yesterday" },
  { label: "1 Week Ago", value: "1 week ago" },
  { label: "1 Month Ago", value: "1 month ago" },
  { label: "1 Year Ago", value: "1 year ago" },
  { label: "Custom", value: "custom" }
];

interface DateValueInputProps {
  value?: DateRangeValue;
  className?: string;
  onChange: (value: DateRangeValue | undefined) => void;
  placeholder?: string;
}

function parseToDate(value?: DateRangeValue): Date | undefined {
  if (!value) return undefined;
  if (value instanceof Date) return value;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

function formatDisplayValue(value?: DateRangeValue, placeholder = "Select date..."): string {
  if (!value) return placeholder;
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
}

const DateValueInput = ({
  className,
  value,
  onChange,
  placeholder = "Select date..."
}: DateValueInputProps) => {
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<string>("calendar");
  const [relativePreset, setRelativePreset] = useState<string>("custom");
  const [customRelative, setCustomRelative] = useState<string>("");
  const [error, setError] = useState<string>("");

  const selectedDate = parseToDate(value);

  const timeValue = selectedDate
    ? `${String(selectedDate.getHours()).padStart(2, "0")}:${String(selectedDate.getMinutes()).padStart(2, "0")}:${String(selectedDate.getSeconds()).padStart(2, "0")}`
    : "12:00:00";

  const handleTimeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const [hours, minutes, seconds] = e.target.value.split(":").map(Number);
    const date = selectedDate ? new Date(selectedDate) : new Date();
    date.setHours(hours ?? 0, minutes ?? 0, seconds ?? 0);
    onChange(date.toISOString());
  };

  const handleDaySelect = (day: Date | undefined) => {
    if (!day) {
      onChange(undefined);
      return;
    }
    const existing = parseToDate(value);
    if (existing) {
      day.setHours(existing.getHours(), existing.getMinutes());
    }
    onChange(day.toISOString());
  };

  const handlePresetChange = (preset: string) => {
    setRelativePreset(preset);
    if (preset !== "custom") {
      onChange(preset);
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
    onChange(trimmed);
    setOpen(false);
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild className={className}>
        <Button variant='outline' className='w-full bg-input/30 font-normal'>
          <CalendarIcon />
          <span className='flex-1 truncate text-left'>
            {formatDisplayValue(value, placeholder)}
          </span>
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
                  id='time-from'
                  type='time'
                  step='1'
                  value={timeValue}
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

export default DateValueInput;
