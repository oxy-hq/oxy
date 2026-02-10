import { Calendar } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
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
import { cn } from "@/libs/utils/cn";
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

function formatDateTimeValue(value?: DateRangeValue): string {
  if (!value) return "";
  if (typeof value === "string") {
    const date = new Date(value);
    if (!Number.isNaN(date.getTime())) {
      return date.toISOString().slice(0, 16);
    }
    return "";
  }
  return value.toISOString().slice(0, 16);
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

  const handleCalendarChange = (dateString: string) => {
    onChange(dateString ? new Date(dateString).toISOString() : undefined);
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
        <button
          type='button'
          className='flex w-full items-center gap-2 rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-xs outline-none transition-[color,box-shadow] hover:bg-accent focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50'
        >
          <Calendar className='h-4 w-4 text-muted-foreground' />
          <span className='flex-1 truncate text-left'>
            {formatDisplayValue(value, placeholder)}
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent className={cn("w-80")} align='start'>
        <Tabs value={tab} onValueChange={setTab}>
          <TabsList className='grid w-full grid-cols-2'>
            <TabsTrigger value='calendar'>Calendar</TabsTrigger>
            <TabsTrigger value='relative'>Relative</TabsTrigger>
          </TabsList>
          <TabsContent value='calendar' className='space-y-2'>
            <Label>Select date and time</Label>
            <Input
              type='datetime-local'
              value={formatDateTimeValue(value)}
              onChange={(e) => handleCalendarChange(e.target.value)}
            />
          </TabsContent>
          <TabsContent value='relative' className='space-y-2'>
            <Label>Relative time</Label>
            <Select value={relativePreset} onValueChange={handlePresetChange}>
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {RELATIVE_PRESETS.map((preset) => (
                  <SelectItem key={preset.value} value={preset.value}>
                    {preset.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            {relativePreset === "custom" && (
              <div className='space-y-2'>
                <Label>Supports chrono-english syntax, e.g. "7 days ago", "3 months ago".</Label>
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
                {error && <p className='text-destructive text-xs'>{error}</p>}
              </div>
            )}
          </TabsContent>
        </Tabs>
      </PopoverContent>
    </Popover>
  );
};

export default DateValueInput;
