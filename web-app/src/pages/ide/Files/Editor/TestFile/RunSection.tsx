import { CirclePlay, Play } from "lucide-react";
import type React from "react";
import { useEffect, useMemo } from "react";
import { Link } from "react-router-dom";
import YAML from "yaml";
import { useFileEditorContext } from "@/components/FileEditor/useFileEditorContext";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import useTestFileResults from "@/stores/useTestFileResults";
import { EvalEventState, MetricKind, type MetricValue } from "@/types/eval";
import type { TestFileFormData } from "./TestFileForm";

interface RunSectionProps {
  pathb64: string;
}

const getScoreDisplay = (metrics: MetricValue[]) => {
  if (metrics.length === 0) return null;
  const metric = metrics[0];
  const score = Math.round(metric.score * 100);
  const passed = score >= 50;
  return { score, passed };
};

const RunSection: React.FC<RunSectionProps> = ({ pathb64 }) => {
  const { state } = useFileEditorContext();
  const { project, branchName } = useCurrentProjectBranch();
  const { runCase, getCase, clearCasesForFile } = useTestFileResults();

  useEffect(() => {
    return () => {
      clearCasesForFile(project.id, branchName, pathb64);
    };
  }, [project.id, branchName, pathb64, clearCasesForFile]);

  const cases = useMemo(() => {
    try {
      if (!state.content) return [];
      const parsed = YAML.parse(state.content) as Partial<TestFileFormData>;
      return parsed.cases ?? [];
    } catch {
      return [];
    }
  }, [state.content]);

  const handleRunAll = () => {
    cases.forEach((_, index) => {
      runCase(project.id, branchName, pathb64, index);
    });
  };

  const handleRunCase = (index: number) => {
    runCase(project.id, branchName, pathb64, index);
  };

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <div className='flex items-center justify-between border-b px-4 py-3'>
        <span className='font-medium text-sm'>Results</span>
        <div className='flex items-center gap-2'>
          <Link
            to={ROUTES.PROJECT(project.id).IDE.TESTS.TEST_FILE(pathb64)}
            className='text-muted-foreground text-xs hover:text-foreground'
          >
            View in Dashboard
          </Link>
          <Button
            variant='outline'
            size='sm'
            className='h-7 gap-1'
            onClick={handleRunAll}
            disabled={cases.length === 0}
          >
            <Play className='h-3 w-3' />
            Run All
          </Button>
        </div>
      </div>
      <div className='flex-1 overflow-y-auto'>
        {cases.length === 0 ? (
          <div className='flex h-full items-center justify-center text-muted-foreground text-sm'>
            No test cases defined
          </div>
        ) : (
          <div className='divide-y'>
            {cases.map((testCase, index) => {
              const caseState = getCase(project.id, branchName, pathb64, index);
              const isRunning =
                caseState.state === EvalEventState.Progress ||
                caseState.state === EvalEventState.Started;
              const scoreDisplay = caseState.result
                ? getScoreDisplay(caseState.result.metrics)
                : null;

              return (
                <div key={index} className='space-y-1 px-4 py-3'>
                  <div className='flex items-start justify-between gap-2'>
                    <p className='line-clamp-2 flex-1 text-sm'>
                      {testCase.prompt || "Empty prompt"}
                    </p>
                    <Button
                      variant='ghost'
                      size='icon'
                      className='h-6 w-6 shrink-0'
                      onClick={() => handleRunCase(index)}
                      disabled={isRunning}
                    >
                      <CirclePlay className='h-3.5 w-3.5' />
                    </Button>
                  </div>
                  {isRunning && (
                    <div className='flex items-center gap-2'>
                      <div className='h-1.5 flex-1 overflow-hidden rounded-full bg-muted'>
                        <div
                          className='h-full rounded-full bg-primary transition-all'
                          style={{
                            width:
                              caseState.progress.total > 0
                                ? `${(caseState.progress.progress / caseState.progress.total) * 100}%`
                                : "0%"
                          }}
                        />
                      </div>
                      <span className='text-muted-foreground text-xs'>
                        {caseState.progress.progress}/{caseState.progress.total}
                      </span>
                    </div>
                  )}
                  {caseState.error && <p className='text-destructive text-xs'>{caseState.error}</p>}
                  {scoreDisplay && (
                    <div className='flex items-center gap-2'>
                      <Badge
                        variant={scoreDisplay.passed ? "default" : "destructive"}
                        className='text-xs'
                      >
                        {scoreDisplay.passed ? "PASS" : "FAIL"}
                      </Badge>
                      <span className='text-muted-foreground text-xs'>{scoreDisplay.score}%</span>
                    </div>
                  )}
                  {caseState.result && caseState.result.metrics.length > 0 && (
                    <div className='mt-1'>
                      {caseState.result.metrics.map((metric, mIdx) => (
                        <div key={mIdx}>
                          {(metric.type === MetricKind.Similarity ||
                            metric.type === MetricKind.Correctness) &&
                            metric.records.map(
                              (record, rIdx) =>
                                record.cot && (
                                  <details key={rIdx} className='mt-1'>
                                    <summary className='cursor-pointer text-muted-foreground text-xs'>
                                      Judge reasoning
                                    </summary>
                                    <p className='mt-1 whitespace-pre-wrap rounded bg-muted p-2 text-xs'>
                                      {record.cot}
                                    </p>
                                  </details>
                                )
                            )}
                        </div>
                      ))}
                    </div>
                  )}
                  {!isRunning &&
                    !caseState.result &&
                    !caseState.error &&
                    caseState.state === null && (
                      <p className='text-muted-foreground text-xs'>Not run yet</p>
                    )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
};

export default RunSection;
