import { Calendar as CalendarIcon } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { cn } from "@/libs/utils/cn";
import { Button } from "./shadcn/button";
import { Calendar } from "./shadcn/calendar";
import { Input } from "./shadcn/input";
import { Popover, PopoverContent, PopoverTrigger } from "./shadcn/popover";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "./shadcn/tabs";

type DateRangePickerProps = {
  value?: [string] | [string, string];
  onChange: (value?: [string] | [string, string]) => void;
  placeholder?: string;
  supportsRelative?: boolean;
};

export const DateRangePicker: React.FC<DateRangePickerProps> = ({
  value,
  onChange,
  placeholder = "Select date range",
  supportsRelative = true
}) => {
  const [mode, setMode] = useState<"absolute" | "relative">("absolute");
  const [relativeExpression, setRelativeExpression] = useState(
    value && value.length === 1 ? value[0] : ""
  );
  const [dateRange, setDateRange] = useState<
    | {
        from: Date | undefined;
        to?: Date | undefined;
      }
    | undefined
  >(() => {
    if (value && value.length === 2) {
      return {
        from: new Date(value[0]),
        to: new Date(value[1])
      };
    }
    return undefined;
  });

  const renderAbsoluteMode = () => (
    <div className='p-3'>
      <Calendar
        mode='range'
        selected={dateRange}
        onSelect={(range) => {
          setDateRange(range);
          if (range?.from) {
            const fromStr = range.from.toISOString().split("T")[0];
            if (range.to) {
              const toStr = range.to.toISOString().split("T")[0];
              onChange([fromStr, toStr]);
            } else {
              onChange([fromStr]);
            }
          } else {
            onChange(undefined);
          }
        }}
        numberOfMonths={2}
      />
    </div>
  );

  const renderRelativeMode = () => (
    <div className='space-y-2 p-3'>
      <Input
        value={relativeExpression}
        onChange={(e) => setRelativeExpression(e.target.value)}
        placeholder='e.g., "last 7 days", "this month", "from 30 days ago to now"'
      />
      <div className='flex gap-2'>
        <Button
          size='sm'
          variant='outline'
          onClick={() => {
            setRelativeExpression("last 7 days");
            onChange(["last 7 days"]);
          }}
        >
          Last 7 days
        </Button>
        <Button
          size='sm'
          variant='outline'
          onClick={() => {
            setRelativeExpression("last 30 days");
            onChange(["last 30 days"]);
          }}
        >
          Last 30 days
        </Button>
        <Button
          size='sm'
          variant='outline'
          onClick={() => {
            setRelativeExpression("this month");
            onChange(["this month"]);
          }}
        >
          This month
        </Button>
      </div>
      <Button
        size='sm'
        className='w-full'
        onClick={() => {
          if (relativeExpression.trim()) {
            onChange([relativeExpression.trim()]);
          }
        }}
      >
        Apply
      </Button>
      <div className='space-y-1 text-muted-foreground text-xs'>
        <p>Examples:</p>
        <ul className='list-inside list-disc'>
          <li>"last 7 days"</li>
          <li>"this month"</li>
          <li>"from 30 days ago to now"</li>
          <li>"from 2023-01-01 to 2023-12-31"</li>
        </ul>
      </div>
    </div>
  );

  const displayValue = () => {
    if (!value) return placeholder;
    if (value.length === 1) {
      return value[0];
    }
    return `${value[0]} to ${value[1]}`;
  };

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          variant='outline'
          className={cn(
            "w-full justify-start text-left font-normal",
            !value && "text-muted-foreground"
          )}
        >
          <CalendarIcon className='mr-2 h-4 w-4' />
          {displayValue()}
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-auto p-0' align='start'>
        {supportsRelative ? (
          <Tabs value={mode} onValueChange={(v) => setMode(v as "absolute" | "relative")}>
            <TabsList className='grid w-full grid-cols-2'>
              <TabsTrigger value='absolute'>Absolute</TabsTrigger>
              <TabsTrigger value='relative'>Relative</TabsTrigger>
            </TabsList>
            <TabsContent value='absolute' className='mt-0'>
              {renderAbsoluteMode()}
            </TabsContent>
            <TabsContent value='relative' className='mt-0'>
              {renderRelativeMode()}
            </TabsContent>
          </Tabs>
        ) : (
          renderAbsoluteMode()
        )}
      </PopoverContent>
    </Popover>
  );
};
