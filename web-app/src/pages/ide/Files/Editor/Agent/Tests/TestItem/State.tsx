import { Check, LoaderCircle } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { TestState } from "@/stores/useTests";
import { EVAL_METRICS_POSTFIX, EvalEventState } from "@/types/eval";
import ProgressIcon from "./ProgressIcon";

const State = ({ testState }: { testState: TestState }) => {
  const { state, progress } = testState;

  const renderState = () => {
    if (!state) return null;

    switch (state) {
      case EvalEventState.Started:
        return (
          <>
            <LoaderCircle className='h-4 w-4 animate-spin' /> Running test
          </>
        );
      case EvalEventState.Progress:
        return progress.id?.endsWith(EVAL_METRICS_POSTFIX) ? (
          <>
            <ProgressIcon progress={progress.progress} total={progress.total} />
            Evaluating records... {progress.progress} / {progress.total}
          </>
        ) : (
          <>
            <ProgressIcon progress={progress.progress} total={progress.total} />
            Generating outputs... {progress.progress} / {progress.total}
          </>
        );
      case EvalEventState.Finished:
        return (
          <>
            <Check className='h-4 w-4' /> Successfully ran test
          </>
        );
      default:
        console.error("Unexpected event state:", state);
        return null;
    }
  };
  return (
    <div
      className={cn(
        "flex items-center justify-center gap-2 rounded-md bg-primary px-4 py-2",
        "text-primary-foreground text-sm"
      )}
    >
      {renderState()}
    </div>
  );
};

export default State;
