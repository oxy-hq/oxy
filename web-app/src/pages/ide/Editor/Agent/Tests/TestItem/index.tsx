import { CirclePlay, LoaderCircle } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { capitalize } from "@/libs/utils/string";
import { Eval } from "@/types/agent";
import { EvalEventState } from "@/types/eval";
import useTests from "@/stores/useTests";
import Result from "./Result";
import State from "./State";

const TestItem = ({
  test,
  agentPathb64,
  index,
}: {
  test: Eval;
  agentPathb64: string;
  index: number;
}) => {
  const { getTest, runTest } = useTests();
  const testState = getTest(agentPathb64, index);
  const { state, result } = testState;

  const isRunning = state && state !== EvalEventState.Finished;

  const handleRunTest = () => {
    runTest(agentPathb64, index);
  };

  return (
    <div className="bg-card-accent rounded-lg">
      <div
        className={cn(
          "flex justify-between gap-1 p-6 border border-border rounded-lg shadow-sm gap-3",
          "hover:bg-card-accent transition-colors",
          "group focus-within:bg-card-accent bg-card",
        )}
      >
        <div className="flex flex-col gap-1 flex-1">
          <Badge variant="secondary" className="w-fit">
            {capitalize(test.type)}
          </Badge>
          <p className="font-semibold text-sm text-left">
            {test.task_description}
          </p>
          {!!test.n && <p className="text-muted-foreground">{test.n} tries</p>}
        </div>
        <div className="flex flex-col gap-2 items-end">
          <Button
            variant="outline"
            onClick={handleRunTest}
            disabled={!!isRunning}
          >
            {isRunning ? (
              <LoaderCircle className="w-4 h-4 animate-spin" />
            ) : (
              <CirclePlay className="w-4 h-4" />
            )}
            Run
          </Button>
          {state && <State testState={testState} />}
        </div>
      </div>

      {result && <Result result={result} />}
    </div>
  );
};

export default TestItem;
