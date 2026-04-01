import { CalendarIcon } from "lucide-react";
import { useRef, useState } from "react";
import { Calendar } from "@/components/ui/shadcn/calendar";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import type { ControlConfig } from "@/types/app";

type Props = {
  control: ControlConfig;
  value: string;
  onChange: (value: string) => void;
};

// Parses a YYYY-MM-DD string into a Date without timezone shift.
function parseDate(str: string): Date | undefined {
  if (!str) return undefined;
  const [y, m, d] = str.split("-").map(Number);
  if (!y || !m || !d) return undefined;
  const date = new Date(y, m - 1, d);
  return Number.isNaN(date.getTime()) ? undefined : date;
}

// Formats a Date to YYYY-MM-DD using local time.
function formatDate(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

export function DateControl({ control, value, onChange }: Props) {
  const [open, setOpen] = useState(false);
  // Tracks the raw text the user is typing — synced to value on valid parse.
  const [text, setText] = useState(value ?? "");
  const inputRef = useRef<HTMLInputElement>(null);

  // Keep text in sync when value changes externally (e.g. on reset).
  if (text !== value && document.activeElement !== inputRef.current) {
    setText(value ?? "");
  }

  const handleTextChange = (raw: string) => {
    setText(raw);
    const date = parseDate(raw);
    if (date) onChange(formatDate(date));
    else if (raw === "") onChange("");
  };

  const handleCalendarSelect = (date: Date | undefined) => {
    const formatted = date ? formatDate(date) : "";
    setText(formatted);
    onChange(formatted);
    setOpen(false);
  };

  const selected = parseDate(value);

  return (
    <div className='flex flex-col gap-1'>
      {control.label && (
        <span className='font-medium text-muted-foreground text-xs'>{control.label}</span>
      )}
      <div className='flex h-8 min-w-36 items-center rounded-md border border-input bg-input/30 text-sm ring-offset-background focus-within:ring-1 focus-within:ring-ring'>
        <input
          ref={inputRef}
          type='text'
          value={text}
          onChange={(e) => handleTextChange(e.target.value)}
          placeholder='YYYY-MM-DD'
          className='min-w-0 flex-1 bg-transparent px-2 py-1 outline-none placeholder:text-muted-foreground'
        />
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <button
              type='button'
              className='flex h-full items-center px-2 text-muted-foreground hover:text-foreground'
            >
              <CalendarIcon className='h-3.5 w-3.5' />
            </button>
          </PopoverTrigger>
          <PopoverContent className='w-auto p-0' align='end'>
            <Calendar
              mode='single'
              selected={selected}
              onSelect={handleCalendarSelect}
              defaultMonth={selected}
              autoFocus
            />
          </PopoverContent>
        </Popover>
      </div>
    </div>
  );
}
