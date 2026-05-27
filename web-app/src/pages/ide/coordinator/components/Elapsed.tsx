import type React from "react";
import { useEffect, useState } from "react";
import { formatDuration, formatDurationMs } from "./utils";

/**
 * Live-ticking elapsed time for an in-flight run. Settled runs get a static
 * duration so the table doesn't churn.
 */
export const Elapsed: React.FC<{
  startIso: string;
  endIso?: string;
  /** When true, the run is still going — tick every second. */
  live?: boolean;
  className?: string;
}> = ({ startIso, endIso, live, className }) => {
  const [, force] = useState(0);

  useEffect(() => {
    if (!live) return;
    const id = setInterval(() => force((n) => n + 1), 1000);
    return () => clearInterval(id);
  }, [live]);

  const text =
    live || !endIso
      ? formatDurationMs(Date.now() - new Date(startIso).getTime())
      : formatDuration(startIso, endIso);

  return <span className={className}>{text}</span>;
};
