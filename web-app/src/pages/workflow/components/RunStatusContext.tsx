/**
 * Per-step run status, scoped to whichever workflow tree is rendered
 * under the provider.
 *
 * The legacy `useWorkflowRun` zustand store kept this state global —
 * which made it leak across navigations and coupled the diagram to a
 * specific event-source shape. This context narrows it to "inside the
 * provider" and reads directly from the live event-stream state
 * already produced by `useWorkflowRunStream` (no zustand involvement).
 *
 * `DiagramNode` calls [`useStepStatus`] as a soft override on top of
 * its legacy `useTaskRun` lookup — when this context is mounted (the
 * new workflow page is) the entry wins; when it's not (the legacy
 * `WorkflowPreview` still uses the diagram), the legacy path keeps
 * working untouched.
 */

import { createContext, type ReactNode, useContext, useMemo } from "react";

import type {
  LiveIteration,
  RunStepState
} from "@/hooks/api/agentic-workflows/useAgenticWorkflows";

export type StepStatus = RunStepState["status"];

export type StepStatusEntry = {
  status: StepStatus;
  errorMessage?: string;
  priorRunId?: string;
  /**
   * Set only for `loop_sequential` steps that have started fanning
   * out. The `LoopProgressBar` reads from this; non-loop nodes can
   * ignore it.
   */
  iterations?: LiveIteration[];
};

/**
 * Per-step replay callback exposed to descendants of the run-status
 * provider. The diagram's per-node "Replay" button calls this on click;
 * it forwards the step name to the controller's `replayStep`, which
 * launches a new run with `invalidate_steps: [stepName]` against the
 * current run id.
 *
 * `null` when the new run page isn't mounted (e.g. legacy
 * `WorkflowPreview` consumer) — the diagram falls back to its own
 * legacy mutation in that case.
 */
type ReplayStepFn = (stepName: string) => Promise<void> | void;

/**
 * Click handler for the loop progress bar. Page-level state holds
 * "which loop is currently selected for the live iteration view in
 * the sidebar"; this callback receives the step name and (typically)
 * sets that state + opens the sidebar's Iterations tab. `null` when
 * the page isn't wiring this (e.g., legacy preview path) — the bar
 * silently falls back to non-clickable.
 */
type SelectLoopFn = (stepName: string) => void;

type ContextValue = {
  statuses: Map<string, StepStatusEntry>;
  replayStep: ReplayStepFn | null;
  selectLoop: SelectLoopFn | null;
};

const RunStatusContext = createContext<ContextValue | null>(null);

type ProviderProps = {
  steps: RunStepState[];
  /**
   * Optional callback invoked by the diagram's per-step replay button.
   * Provider supplies it; consumers reach it via `useReplayStep`. When
   * absent, `useReplayStep` returns `null` and the diagram falls back to
   * its legacy replay mutation.
   */
  onReplayStep?: ReplayStepFn;
  /** See [`SelectLoopFn`]. */
  onSelectLoop?: SelectLoopFn;
  children: ReactNode;
};

export const RunStatusProvider = ({
  steps,
  onReplayStep,
  onSelectLoop,
  children
}: ProviderProps) => {
  const value = useMemo<ContextValue>(() => {
    const statuses = new Map<string, StepStatusEntry>();
    for (const step of steps) {
      statuses.set(step.name, {
        status: step.status,
        errorMessage: step.errorMessage,
        priorRunId: step.priorRunId,
        iterations: step.iterations
      });
    }
    return {
      statuses,
      replayStep: onReplayStep ?? null,
      selectLoop: onSelectLoop ?? null
    };
  }, [steps, onReplayStep, onSelectLoop]);

  return <RunStatusContext.Provider value={value}>{children}</RunStatusContext.Provider>;
};

/**
 * Look up the live status for a step by name. Returns `undefined`
 * when no provider is mounted *or* when the step hasn't appeared in
 * the stream yet — callers should treat both as "no override, render
 * default."
 */
export const useStepStatus = (stepName: string): StepStatusEntry | undefined => {
  const ctx = useContext(RunStatusContext);
  return ctx?.statuses.get(stepName);
};

/**
 * Returns the provider-supplied replay callback (or `null` when no
 * provider / no callback was supplied). Consumers call it with the step
 * name they want to re-run.
 */
export const useReplayStep = (): ReplayStepFn | null => {
  const ctx = useContext(RunStatusContext);
  return ctx?.replayStep ?? null;
};

/**
 * Returns the provider-supplied select-loop callback (or `null`).
 * The loop diagram node wires its `LoopProgressBar`'s `onClick` to
 * this so the page can toggle the sidebar's live iteration view.
 */
export const useSelectLoop = (): SelectLoopFn | null => {
  const ctx = useContext(RunStatusContext);
  return ctx?.selectLoop ?? null;
};
