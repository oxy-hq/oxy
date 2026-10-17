import { BellOff } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import type { GlobalNotificationSettings } from "@/types/notifications";
import { formatUntil, PAUSE_PRESETS, pausedUntil } from "../schedule";

interface PauseRowProps {
  global: GlobalNotificationSettings;
  /** An ISO instant to pause until, or `null` to resume. */
  onChange: (pausedUntil: string | null) => void;
}

/**
 * Hold back push, email and Slack for a while — everything but critical. The
 * inbox still gets it all. Paused, it says until when, with a way back.
 */
export function PauseRow({ global, onChange }: PauseRowProps) {
  const now = new Date();
  const until = pausedUntil(global, now);

  return (
    <div className='flex items-center gap-3 px-4 py-3' data-testid='notifications-pause'>
      <BellOff className='size-4 shrink-0 text-muted-foreground' aria-hidden />
      <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <span className='text-sm' data-testid='notifications-pause-status'>
          {until ? `Paused until ${formatUntil(until, now)}` : "Pause notifications"}
        </span>
        {until && (
          <span className='text-muted-foreground text-xs'>Critical still comes through</span>
        )}
      </div>
      {until ? (
        <Button
          size='sm'
          variant='outline'
          onClick={() => onChange(null)}
          data-testid='notifications-pause-resume'
        >
          Resume
        </Button>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size='sm' variant='outline' data-testid='notifications-pause-open'>
              Pause
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='end'>
            {PAUSE_PRESETS.map((preset) => (
              <DropdownMenuItem
                key={preset.id}
                onSelect={() => onChange(preset.until(new Date()).toISOString())}
                data-testid={`notifications-pause-${preset.id}`}
              >
                {preset.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}
