import { useEffect, useMemo, useState } from "react";
import useWorkspaceTimezone from "@/hooks/api/agents/useWorkspaceTimezone";

/**
 * Live clock in the workspace's timezone (default `America/Los_Angeles`), not
 * UTC — derived from the default agent's `.agentic.yml` `timezone:`. Updates
 * each half-minute (minute precision), pausing when the tab is hidden.
 */
export function WorkspaceClock() {
  const tz = useWorkspaceTimezone();
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const update = () => setNow(new Date());
    const id = setInterval(() => {
      if (!document.hidden) update();
    }, 30_000);
    // Re-show: tick immediately so the clock isn't up to 30s stale after the
    // tab was backgrounded (interval ticks are skipped while hidden).
    const onVisible = () => {
      if (!document.hidden) update();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);

  const text = useMemo(() => {
    try {
      return new Intl.DateTimeFormat(undefined, {
        timeZone: tz,
        hour: "numeric",
        minute: "2-digit",
        timeZoneName: "short"
      }).format(now);
    } catch {
      // Invalid IANA name → local time without a zone label rather than crash.
      return new Intl.DateTimeFormat(undefined, {
        hour: "numeric",
        minute: "2-digit"
      }).format(now);
    }
  }, [now, tz]);

  return (
    <span
      data-testid='topbar-clock'
      className='hidden font-mono text-muted-foreground text-xs tabular-nums sm:inline'
    >
      {text}
    </span>
  );
}
