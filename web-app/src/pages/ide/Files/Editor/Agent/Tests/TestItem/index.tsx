import { CirclePlay, LoaderCircle } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { cn } from "@/libs/shadcn/utils";
import { capitalize } from "@/libs/utils/string";
import useTests from "@/stores/useTests";
import type { Eval } from "@/types/agent";
import { EvalEventState } from "@/types/eval";
import Result from "./Result";
import State from "./State";

const TestItem = ({
  test,
  agentPathb64,
  index
}: {
  test: Eval;
  agentPathb64: string;
  index: number;
}) => {
  const { getTest, runTest } = useTests();
  const { project, branchName } = useCurrentProjectBranch();
  const testState = getTest(project.id, branchName, agentPathb64, index);
  const { state, result } = testState;

  const isRunning = state && state !== EvalEventState.Finished;

  const handleRunTest = () => {
    runTest(project.id, branchName, agentPathb64, index);
  };

  return (
    <div className='rounded-lg bg-card-accent'>
      <div
        className={cn(
          "flex justify-between gap-1 gap-3 rounded-lg border border-border p-6 shadow-sm",
          "transition-colors hover:bg-card-accent",
          "group bg-card focus-within:bg-card-accent"
        )}
      >
        <div className='flex flex-1 flex-col gap-1'>
          <Badge variant='secondary' className='w-fit'>
            {capitalize(test.type)}
          </Badge>
          <p className='text-left font-semibold text-sm'>{test.task_description}</p>
          {!!test.n && <p className='text-muted-foreground'>{test.n} tries</p>}
        </div>
        <div className='flex flex-col items-end gap-2'>
          <Button variant='outline' onClick={handleRunTest} disabled={!!isRunning}>
            {isRunning ? (
              <LoaderCircle className='h-4 w-4 animate-spin' />
            ) : (
              <CirclePlay className='h-4 w-4' />
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
