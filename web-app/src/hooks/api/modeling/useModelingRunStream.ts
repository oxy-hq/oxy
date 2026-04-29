import { useCallback, useRef, useState } from "react";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { ModelingService } from "@/services/api/modeling";
import type { RunStreamEvent } from "@/types/modeling";

export type RunStreamState =
  | { phase: "idle" }
  | { phase: "running"; events: RunStreamEvent[] }
  | { phase: "done"; events: RunStreamEvent[] }
  | { phase: "error"; events: RunStreamEvent[]; message: string };

export default function useModelingRunStream(modelingProjectName: string) {
  const { project, branchName } = useCurrentProjectBranch();
  const [state, setState] = useState<RunStreamState>({ phase: "idle" });
  const abortRef = useRef<AbortController | null>(null);

  const run = useCallback(
    async (selector?: string) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      setState({ phase: "running", events: [] });

      try {
        await ModelingService.runModelsStream(
          project.id,
          modelingProjectName,
          { selector },
          branchName,
          (event) => {
            setState((prev) => {
              const events = prev.phase !== "idle" ? [...prev.events, event] : [event];
              if (event.kind === "done") return { phase: "done", events };
              if (event.kind === "error") return { phase: "error", events, message: event.message };
              return { phase: "running", events };
            });
          },
          controller.signal
        );
      } catch (err) {
        if ((err as Error)?.name === "AbortError") return;
        setState((prev) => ({
          phase: "error",
          events: prev.phase !== "idle" ? prev.events : [],
          message: err instanceof Error ? err.message : "Unknown error"
        }));
      }
    },
    [project.id, modelingProjectName, branchName]
  );

  const abort = useCallback(() => {
    abortRef.current?.abort();
    setState((prev) => ({
      phase: "done",
      events: prev.phase !== "idle" ? prev.events : []
    }));
  }, []);

  const reset = useCallback(() => {
    setState({ phase: "idle" });
  }, []);

  return { state, run, abort, reset };
}
