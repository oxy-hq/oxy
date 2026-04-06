import { AlertCircle, History, Play, Trash2 } from "lucide-react";
import type React from "react";
import { useNavigate } from "react-router-dom";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useDeleteTestProjectRun, useTestProjectRuns } from "@/hooks/api/tests/useTestProjectRuns";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import ROUTES from "@/libs/utils/routes";
import PageHeader from "@/pages/ide/components/PageHeader";

const formatDate = (iso: string) =>
  new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  });

const formatDuration = (ms: number) => {
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s`;
};

const formatTokens = (n: number) => {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
};

const scoreClass = (pct: number) =>
  pct >= 80
    ? "border-success text-success"
    : pct >= 50
      ? "border-warning text-warning"
      : "border-destructive text-destructive";

const ScoreBadge: React.FC<{ score: number | null; failed?: boolean }> = ({ score, failed }) => {
  if (failed) {
    return (
      <Badge variant='outline' className='gap-1 border-destructive/50 text-destructive text-xs'>
        <AlertCircle className='h-3 w-3 text-destructive' />
        Failed
      </Badge>
    );
  }
  if (score === null) return <span className='text-muted-foreground text-xs'>—</span>;
  const pct = Math.round(score * 100);
  return (
    <Badge variant='outline' className={`text-xs tabular-nums ${scoreClass(pct)}`}>
      {pct}%
    </Badge>
  );
};

const Dash = () => <span className='text-muted-foreground text-xs'>—</span>;

const TestsRunsPage: React.FC = () => {
  const { project } = useCurrentProjectBranch();
  const projectId = project.id;
  const { data: runs, isLoading } = useTestProjectRuns();
  const deleteRun = useDeleteTestProjectRun();
  const navigate = useNavigate();

  const handleView = (runId: string) => {
    navigate(`${ROUTES.PROJECT(projectId).IDE.TESTS.ROOT}?run_id=${runId}`);
  };

  return (
    <div className='flex h-full flex-col'>
      <PageHeader icon={History} title='Runs' />
      <div className='min-h-0 flex-1 overflow-auto p-4'>
        {isLoading && (
          <div className='space-y-2'>
            {Array.from({ length: 5 }).map((_, i) => (
              <Skeleton key={i} className='h-12 w-full' />
            ))}
          </div>
        )}

        {!isLoading && (!runs || runs.length === 0) && (
          <div className='flex flex-col items-center justify-center rounded-lg border border-dashed p-12 text-center'>
            <History className='mb-3 h-10 w-10 text-muted-foreground' />
            <p className='font-medium text-sm'>No runs yet</p>
            <p className='mt-1 text-muted-foreground text-xs'>
              Use <strong>Run All</strong> on the Dashboard to create your first test suite run.
            </p>
          </div>
        )}

        {!isLoading && runs && runs.length > 0 && (
          <div className='overflow-hidden rounded-lg border'>
            <table className='w-full text-sm'>
              <thead>
                <tr className='border-b bg-muted/40 text-left text-muted-foreground text-xs'>
                  <th className='px-4 py-2 font-medium'>Run</th>
                  <th className='px-4 py-2 font-medium'>Date</th>
                  <th className='px-4 py-2 font-medium'>Cases</th>
                  <th className='px-4 py-2 font-medium'>Total Time</th>
                  <th className='px-4 py-2 font-medium'>Tokens</th>
                  <th className='px-4 py-2 font-medium'>Consistency</th>
                  <th className='px-4 py-2 font-medium'>Score</th>
                  <th className='px-4 py-2'></th>
                </tr>
              </thead>
              <tbody className='divide-y'>
                {runs.map((run, i) => (
                  <tr key={run.id} className='group transition-colors hover:bg-muted/30'>
                    <td className='px-4 py-2.5'>
                      <div className='flex items-center gap-2'>
                        <span className='font-medium'>{run.name ?? `Run #${runs.length - i}`}</span>
                        {i === 0 && (
                          <Badge variant='secondary' className='text-[10px]'>
                            latest
                          </Badge>
                        )}
                      </div>
                    </td>
                    <td className='px-4 py-2.5 text-muted-foreground text-xs tabular-nums'>
                      {formatDate(run.created_at)}
                    </td>
                    <td className='px-4 py-2.5 text-muted-foreground text-xs tabular-nums'>
                      {run.total_cases !== null ? run.total_cases : <Dash />}
                    </td>
                    <td className='px-4 py-2.5 text-muted-foreground text-xs tabular-nums'>
                      {run.total_duration_ms !== null ? (
                        formatDuration(run.total_duration_ms)
                      ) : (
                        <Dash />
                      )}
                    </td>
                    <td className='px-4 py-2.5 text-muted-foreground text-xs tabular-nums'>
                      {run.total_tokens !== null ? formatTokens(run.total_tokens) : <Dash />}
                    </td>
                    <td className='px-4 py-2.5 text-muted-foreground text-xs tabular-nums'>
                      {run.consistency !== null ? (
                        `${Math.round(run.consistency * 100)}%`
                      ) : (
                        <Dash />
                      )}
                    </td>
                    <td className='px-4 py-2.5'>
                      <ScoreBadge
                        score={run.score}
                        failed={
                          run.score === null &&
                          run.total_cases === null &&
                          run.file_scores.length > 0
                        }
                      />
                    </td>
                    <td className='px-4 py-2.5'>
                      <div className='flex items-center justify-end gap-1 opacity-0 transition-opacity group-hover:opacity-100'>
                        <Button
                          variant='ghost'
                          size='sm'
                          className='h-7 gap-1 text-xs'
                          onClick={() => handleView(run.id)}
                        >
                          <Play className='h-3 w-3' />
                          View
                        </Button>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-7 w-7 text-muted-foreground hover:text-destructive'
                          onClick={() => deleteRun.mutate({ projectRunId: run.id })}
                          disabled={deleteRun.isPending}
                        >
                          <Trash2 className='h-3 w-3' />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};

export default TestsRunsPage;
