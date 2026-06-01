/**
 * Extract → Normalize → Load phase indicator for an airway run.
 *
 * Presentation only — driven entirely by `AirwayRunView.phase`. A
 * pipeline isn't a DAG; the three fixed phases are the whole story, so
 * this is a fixed three-segment bar rather than a node graph.
 */

import { Check, Loader2 } from "lucide-react";
import type React from "react";

import { cn } from "@/libs/shadcn/utils";
import type { AirwayRunView, PhaseState } from "@/utils/airwayReducer";

const PHASES: { key: keyof AirwayRunView["phase"]; label: string }[] = [
  { key: "extract", label: "Extract" },
  { key: "normalize", label: "Normalize" },
  { key: "load", label: "Load" }
];

const dotClass: Record<PhaseState, string> = {
  pending: "border-border bg-background text-muted-foreground",
  active: "border-primary bg-primary/10 text-primary",
  done: "border-primary bg-primary text-primary-foreground"
};

const PhaseDot: React.FC<{ state: PhaseState }> = ({ state }) => (
  <span
    className={cn("flex h-6 w-6 items-center justify-center rounded-full border", dotClass[state])}
  >
    {state === "done" ? (
      <Check className='h-3.5 w-3.5' />
    ) : state === "active" ? (
      <Loader2 className='h-3.5 w-3.5 animate-spin' />
    ) : null}
  </span>
);

type Props = {
  phase: AirwayRunView["phase"];
  loadId?: string;
};

const PhaseBar: React.FC<Props> = ({ phase, loadId }) => (
  <div className='flex items-center gap-3 px-4 py-3'>
    {PHASES.map(({ key, label }, i) => {
      const state = phase[key];
      return (
        <div key={key} className='flex items-center gap-3'>
          <div className='flex items-center gap-2'>
            <PhaseDot state={state} />
            <span
              className={cn(
                "text-sm",
                state === "pending" ? "text-muted-foreground" : "font-medium text-foreground"
              )}
            >
              {label}
            </span>
          </div>
          {i < PHASES.length - 1 && (
            <span className={cn("h-px w-8", state === "done" ? "bg-primary" : "bg-border")} />
          )}
        </div>
      );
    })}
    {loadId && (
      <span className='ml-auto font-mono text-muted-foreground text-xs'>load {loadId}</span>
    )}
  </div>
);

export default PhaseBar;
