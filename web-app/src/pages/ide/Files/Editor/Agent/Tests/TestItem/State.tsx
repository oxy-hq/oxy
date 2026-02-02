import { Check, LoaderCircle } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { EVAL_METRICS_POSTFIX, EvalEventState } from "@/types/eval";
import { TestState } from "@/stores/useTests";
import ProgressIcon from "./ProgressIcon";

const State = ({ testState }: { testState: TestState }) => {
  const { state, progress } = testState;

  const renderState = () => {
    if (!state) return null;

    switch (state) {
      case EvalEventState.Started:
        return (
          <>
            <LoaderCircle className="w-4 h-4 animate-spin" /> Running test
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
            <Check className="w-4 h-4" /> Successfully ran test
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
        "flex gap-2 px-4 py-2 justify-center items-center rounded-md bg-primary",
        "text-primary-foreground text-sm",
      )}
    >
      {renderState()}
    </div>
  );
};

export default State;
