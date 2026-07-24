import type { ReactNode } from "react";
import { useEffect, useState } from "react";

// A live wall clock in the viewer's local timezone, ticking every 30s. The
// "live · HH:MM" chip signals a running operational surface rather than a
// static page.
function useLocalClock(): string {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(id);
  }, []);
  return now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
}

// The 36px top bar: OXYGEN wordmark · breadcrumb · live status.
export function Topbar({ breadcrumb }: { breadcrumb: ReactNode }) {
  const time = useLocalClock();
  return (
    <header className='flex h-9 shrink-0 items-center gap-3.5 border-border border-b bg-background px-3.5 text-[11px]'>
      <span className='font-semibold text-foreground tracking-wider'>OXYGEN</span>
      <span className='text-muted-foreground'>{breadcrumb}</span>
      <div className='ml-auto flex items-center gap-2.5 font-mono text-[10px] text-muted-foreground'>
        <span
          role='img'
          aria-label='live'
          className='size-1.5 rounded-full bg-status-success shadow-[0_0_6px_var(--color-status-success)]'
        />
        <span>live · {time}</span>
      </div>
    </header>
  );
}
