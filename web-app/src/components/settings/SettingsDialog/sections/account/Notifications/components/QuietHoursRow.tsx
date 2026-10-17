import { Moon } from "lucide-react";
import { type KeyboardEvent, useId, useState } from "react";
import { Checkbox } from "@/components/ui/shadcn/checkbox";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import { Switch } from "@/components/ui/shadcn/switch";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/shadcn/toggle-group";
import type { GlobalNotificationSettings, QuietHours } from "@/types/notifications";
import { DAYS, DEFAULT_QUIET_HOURS, quietHoursChange, quietHoursError } from "../schedule";

interface QuietHoursRowProps {
  global: GlobalNotificationSettings;
  onChange: (change: Partial<GlobalNotificationSettings>) => void;
}

/**
 * A daily window when push and Slack hold back anything below critical (or
 * below time-sensitive, if let through). Email is not held: it does not
 * interrupt. Times are in the stored time zone, set from this browser's the
 * first time quiet hours are saved.
 */
export function QuietHoursRow({ global, onChange }: QuietHoursRowProps) {
  const quiet = global.quiet_hours;
  const save = (next: QuietHours | null) => onChange(quietHoursChange(next, global));

  return (
    <div className='flex flex-col gap-3 px-4 py-3' data-testid='notifications-quiet'>
      <div className='flex items-center gap-3'>
        <Moon className='size-4 shrink-0 text-muted-foreground' aria-hidden />
        <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
          <span className='text-sm'>Quiet hours</span>
          {quiet && global.timezone && (
            <span
              className='text-muted-foreground text-xs'
              data-testid='notifications-quiet-timezone'
            >
              {global.timezone}
            </span>
          )}
        </div>
        <Switch
          checked={quiet !== null}
          onCheckedChange={(on) => save(on ? DEFAULT_QUIET_HOURS : null)}
          aria-label='Quiet hours'
          data-testid='notifications-quiet-toggle'
        />
      </div>
      {quiet && (
        <QuietHoursEditor key={`${quiet.start}-${quiet.end}`} quiet={quiet} onSave={save} />
      )}
    </div>
  );
}

const blurOnEnter = (e: KeyboardEvent<HTMLInputElement>) => {
  if (e.key === "Enter") e.currentTarget.blur();
};

/** Start and end save when the field is left; days and the time-sensitive box save at once. */
function QuietHoursEditor({
  quiet,
  onSave
}: {
  quiet: QuietHours;
  onSave: (next: QuietHours) => void;
}) {
  const allowId = useId();
  const [start, setStart] = useState(quiet.start);
  const [end, setEnd] = useState(quiet.end);
  const times = { ...quiet, start, end };
  const error = quietHoursError(times);
  const commitTimes = () => {
    if (!error && (start !== quiet.start || end !== quiet.end)) onSave(times);
  };

  return (
    <div className='flex flex-col gap-3 pl-7'>
      <div className='flex flex-wrap items-center gap-2'>
        <Input
          type='time'
          value={start}
          onChange={(e) => setStart(e.target.value)}
          onBlur={commitTimes}
          onKeyDown={blurOnEnter}
          aria-label='Start'
          aria-invalid={error !== null}
          className='h-8 w-32'
          data-testid='notifications-quiet-start'
        />
        <span className='text-muted-foreground text-sm'>to</span>
        <Input
          type='time'
          value={end}
          onChange={(e) => setEnd(e.target.value)}
          onBlur={commitTimes}
          onKeyDown={blurOnEnter}
          aria-label='End'
          aria-invalid={error !== null}
          className='h-8 w-32'
          data-testid='notifications-quiet-end'
        />
      </div>
      {error && (
        <p className='text-destructive text-xs' data-testid='notifications-quiet-error'>
          {error}
        </p>
      )}
      <ToggleGroup
        type='multiple'
        variant='outline'
        size='sm'
        className='justify-start'
        value={quiet.days.map(String)}
        onValueChange={(days) => {
          // At least one day: the last one cannot be turned off.
          if (days.length > 0) onSave({ ...quiet, days: days.map(Number).sort((a, b) => a - b) });
        }}
      >
        {DAYS.map((day) => (
          <ToggleGroupItem
            key={day.value}
            value={String(day.value)}
            aria-label={day.label}
            className='data-[state=on]:bg-primary data-[state=on]:text-primary-foreground'
            data-testid={`notifications-quiet-day-${day.value}`}
          >
            {day.short}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <div className='flex items-center gap-2'>
        <Checkbox
          id={allowId}
          checked={quiet.allow === "time_sensitive"}
          onCheckedChange={(checked) =>
            onSave({ ...quiet, allow: checked === true ? "time_sensitive" : "critical" })
          }
          data-testid='notifications-quiet-allow'
        />
        <Label htmlFor={allowId} className='font-normal text-sm'>
          Let time-sensitive through
        </Label>
      </div>
    </div>
  );
}
