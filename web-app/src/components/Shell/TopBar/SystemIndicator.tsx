import useOnlineStatus from "@/hooks/useOnlineStatus";
import { cn } from "@/libs/shadcn/utils";

/**
 * Universal "Sys: Connected" indicator — a pulsing green dot when the browser
 * is online, red when offline. Presentational connectivity signal borrowed
 * from the Command Center top bar; a real backend health ping can replace
 * `useOnlineStatus` later without touching consumers.
 */
export function SystemIndicator() {
  const online = useOnlineStatus();
  return (
    <span
      data-testid='topbar-system'
      className='hidden items-center gap-1.5 font-mono text-muted-foreground text-xs sm:inline-flex'
    >
      <span>Sys:</span>
      <span
        className={cn(
          "inline-flex items-center gap-1",
          online ? "text-success" : "text-destructive"
        )}
      >
        <span className='relative inline-flex size-1.5'>
          {online && (
            <span className='absolute inline-flex size-1.5 animate-ping rounded-full bg-success opacity-75' />
          )}
          <span
            className={cn(
              "relative inline-flex size-1.5 rounded-full",
              online ? "bg-success" : "bg-destructive"
            )}
          />
        </span>
        {online ? "Connected" : "Offline"}
      </span>
    </span>
  );
}
