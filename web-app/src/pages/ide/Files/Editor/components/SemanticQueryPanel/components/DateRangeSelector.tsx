import { Calendar } from "lucide-react";
import { useState } from "react";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
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

const DateRangeSelector = ({ from, to, onChange, className }: DateRangeSelectorProps) => {
  const [fromOpen, setFromOpen] = useState(false);
  const [toOpen, setToOpen] = useState(false);
  const [fromTab, setFromTab] = useState<string>("calendar");
  const [toTab, setToTab] = useState<string>("calendar");
  const [fromRelativePreset, setFromRelativePreset] = useState<string>("custom");
  const [toRelativePreset, setToRelativePreset] = useState<string>("custom");
  const [fromCustomRelative, setFromCustomRelative] = useState<string>("");
  const [toCustomRelative, setToCustomRelative] = useState<string>("");
  const [fromError, setFromError] = useState<string>("");
  const [toError, setToError] = useState<string>("");

  const formatDateTimeValue = (value?: DateRangeValue): string => {
    if (!value) return "";
    if (typeof value === "string") {
      // Try to parse as ISO date
      const date = new Date(value);
      if (!Number.isNaN(date.getTime())) {
        return date.toISOString().slice(0, 16);
      }
      return "";
    }
    return value.toISOString().slice(0, 16);
  };

  const formatDisplayValue = (value?: DateRangeValue): string => {
    if (!value) return "Select...";
    if (typeof value === "string") {
      // Try to parse as ISO date
      const date = new Date(value);
      if (!Number.isNaN(date.getTime())) {
        return new Intl.DateTimeFormat("en-US", {
          dateStyle: "medium",
          timeStyle: "short"
        }).format(date);
      }
      // Otherwise show the relative string
      return value;
    }
    return new Intl.DateTimeFormat("en-US", {
      dateStyle: "medium",
      timeStyle: "short"
    }).format(value);
  };

  const handleFromCalendarChange = (dateString: string) => {
    onChange({
      relative: undefined,
      from: dateString ? new Date(dateString).toISOString() : undefined,
      to
    });
  };

  const handleToCalendarChange = (dateString: string) => {
    onChange({
      relative: undefined,
      from,
      to: dateString ? new Date(dateString).toISOString() : undefined
    });
  };

  const handleFromRelativePresetChange = (value: string) => {
    setFromRelativePreset(value);
    if (value !== "custom") {
      onChange({
        relative: undefined,
        from: value,
        to
      });
      setFromOpen(false);
    }
  };

  const handleToRelativePresetChange = (value: string) => {
    setToRelativePreset(value);
    if (value !== "custom") {
      onChange({
        relative: undefined,
        from,
        to: value
      });
      setToOpen(false);
    }
  };

  const handleFromCustomRelativeApply = () => {
    const trimmed = fromCustomRelative.trim();
    if (!trimmed) {
      setFromError("Please enter a value");
      return;
    }

    setFromError("");
    onChange({
      relative: undefined,
      from: trimmed,
      to
    });
    setFromOpen(false);
  };

  const handleToCustomRelativeApply = () => {
    const trimmed = toCustomRelative.trim();
    if (!trimmed) {
      setToError("Please enter a value");
      return;
    }

    setToError("");
    onChange({
      relative: undefined,
      from,
      to: trimmed
    });
    setToOpen(false);
  };

  return (
    <div className='flex h-full flex-1 items-center gap-2'>
      <Popover open={fromOpen} onOpenChange={setFromOpen}>
        <PopoverTrigger asChild className={className}>
          <button
            type='button'
            className='flex flex-1 items-center gap-1 rounded border bg-background px-2 py-1 text-left text-xs hover:bg-muted'
          >
            <Calendar className='w-3' />
            <span className='flex-1 truncate'>{formatDisplayValue(from)}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent className='w-80' align='start'>
          <Tabs value={fromTab} onValueChange={setFromTab}>
            <TabsList className='grid w-full grid-cols-2'>
              <TabsTrigger value='calendar'>Calendar</TabsTrigger>
              <TabsTrigger value='relative'>Relative</TabsTrigger>
            </TabsList>
            <TabsContent value='calendar' className='space-y-2'>
              <Label htmlFor='from-datetime'>Select date and time</Label>
              <Input
                id='from-datetime'
                type='datetime-local'
                value={formatDateTimeValue(from)}
                onChange={(e) => handleFromCalendarChange(e.target.value)}
                className='text-xs'
              />
            </TabsContent>
            <TabsContent value='relative' className='space-y-2'>
              <Label htmlFor='from-relative-preset'>Relative time</Label>
              <select
                id='from-relative-preset'
                value={fromRelativePreset}
                onChange={(e) => handleFromRelativePresetChange(e.target.value)}
                className='w-full rounded border bg-background px-2 py-1 text-xs'
              >
                {RELATIVE_PRESETS.map((preset) => (
                  <option key={preset.value} value={preset.value}>
                    {preset.label}
                  </option>
                ))}
              </select>
              {fromRelativePreset === "custom" && (
                <div className='space-y-2'>
                  <Label htmlFor='from-custom-relative'>
                    We support chrono-english syntax. E.g., "7 days ago", "3 months ago".
                  </Label>
                  <div className='flex gap-2'>
                    <Input
                      id='from-custom-relative'
                      type='text'
                      placeholder='e.g., 7 days ago'
                      value={fromCustomRelative}
                      onChange={(e) => {
                        setFromCustomRelative(e.target.value);
                        if (fromError) setFromError("");
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          handleFromCustomRelativeApply();
                        }
                      }}
                      className='text-xs'
                    />
                    <button
                      type='button'
                      onClick={handleFromCustomRelativeApply}
                      className='rounded border bg-primary px-3 py-1 text-primary-foreground text-xs hover:bg-primary/90'
                    >
                      Apply
                    </button>
                  </div>
                  {fromError && <p className='text-destructive text-xs'>{fromError}</p>}
                </div>
              )}
            </TabsContent>
          </Tabs>
        </PopoverContent>
      </Popover>

      <span className='text-muted-foreground text-xs'>→</span>

      <Popover open={toOpen} onOpenChange={setToOpen}>
        <PopoverTrigger asChild className={className}>
          <button
            type='button'
            className='flex flex-1 items-center gap-1 rounded border bg-background px-2 py-1 text-left text-xs hover:bg-muted'
          >
            <Calendar className='w-3' />
            <span className='flex-1 truncate'>{formatDisplayValue(to)}</span>
          </button>
        </PopoverTrigger>
        <PopoverContent className='w-80' align='start'>
          <Tabs value={toTab} onValueChange={setToTab}>
            <TabsList className='grid w-full grid-cols-2'>
              <TabsTrigger value='calendar'>Calendar</TabsTrigger>
              <TabsTrigger value='relative'>Relative</TabsTrigger>
            </TabsList>
            <TabsContent value='calendar' className='space-y-2'>
              <Label htmlFor='to-datetime'>Select date and time</Label>
              <Input
                id='to-datetime'
                type='datetime-local'
                value={formatDateTimeValue(to)}
                onChange={(e) => handleToCalendarChange(e.target.value)}
                className='text-xs'
              />
            </TabsContent>
            <TabsContent value='relative' className='space-y-2'>
              <Label htmlFor='to-relative-preset'>Relative time</Label>
              <select
                id='to-relative-preset'
                value={toRelativePreset}
                onChange={(e) => handleToRelativePresetChange(e.target.value)}
                className='w-full rounded border bg-background px-2 py-1 text-xs'
              >
                {RELATIVE_PRESETS.map((preset) => (
                  <option key={preset.value} value={preset.value}>
                    {preset.label}
                  </option>
                ))}
              </select>
              {toRelativePreset === "custom" && (
                <div className='space-y-2'>
                  <Label htmlFor='to-custom-relative'>
                    Custom expression (e.g., "now", "today", "3 hours ago")
                  </Label>
                  <div className='flex gap-2'>
                    <Input
                      id='to-custom-relative'
                      type='text'
                      placeholder='e.g., now'
                      value={toCustomRelative}
                      onChange={(e) => {
                        setToCustomRelative(e.target.value);
                        if (toError) setToError("");
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          handleToCustomRelativeApply();
                        }
                      }}
                      className='text-xs'
                    />
                    <button
                      type='button'
                      onClick={handleToCustomRelativeApply}
                      className='rounded border bg-primary px-3 py-1 text-primary-foreground text-xs hover:bg-primary/90'
                    >
                      Apply
                    </button>
                  </div>
                  {toError && <p className='text-destructive text-xs'>{toError}</p>}
                </div>
              )}
            </TabsContent>
          </Tabs>
        </PopoverContent>
      </Popover>
    </div>
  );
};

export default DateRangeSelector;
