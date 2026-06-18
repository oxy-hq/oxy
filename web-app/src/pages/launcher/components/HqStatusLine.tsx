import { Fragment } from "react";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useHqAlerts } from "./useHqAlerts";

const plural = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;

/** A compact operational status line under the HQ heading — a calm signal of
 *  health and scale, not a dashboard (Command Center owns live operations).
 *  App and signal counts are live; there's no store/source vitals feed yet, so
 *  today it shows `● Operational · N apps`. */
export function HqStatusLine() {
  const { project } = useCurrentProjectBranch();
  const { data: apps = [] } = useCustomApps(project?.id ?? "");
  const signals = useHqAlerts();

  const facts: { text: string }[] = [
    ...(apps.length > 0 ? [{ text: plural(apps.length, "app") }] : []),
    ...(signals.length > 0 ? [{ text: plural(signals.length, "signal") }] : [])
  ];

  return (
    <div
      className='flex flex-wrap items-center gap-x-1.5 text-muted-foreground/70 text-xs'
      data-testid='hq-status-line'
    >
      <span className='inline-block h-1.5 w-1.5 rounded-full bg-green-500' aria-hidden='true' />
      <span>Operational</span>
      {facts.map((fact) => (
        <Fragment key={fact.text}>
          <span className='text-muted-foreground/30'>·</span>
          <span>{fact.text}</span>
        </Fragment>
      ))}
    </div>
  );
}
