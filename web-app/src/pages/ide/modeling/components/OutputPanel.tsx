import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type { RunStreamState } from "@/hooks/api/modeling/useModelingRunStream";
import { cn } from "@/libs/shadcn/utils";
import type {
  AnalyzeOutput,
  CompileOutput,
  NodeRunResult,
  RunOutput,
  RunStreamEvent,
  SeedOutput,
  TestOutput
} from "@/types/modeling";

type OutputState =
  | { kind: "compile"; data: CompileOutput }
  | { kind: "run"; data: RunOutput }
  | { kind: "test"; data: TestOutput }
  | { kind: "analyze"; data: AnalyzeOutput }
  | { kind: "seed"; data: SeedOutput }
  | { kind: "error"; message: string }
  | null;

interface OutputPanelProps {
  output: OutputState;
  isPending: boolean;
  runStream?: RunStreamState;
}

const ExpandableErrorText: React.FC<{ message: string }> = ({ message }) => {
  const [expanded, setExpanded] = useState(false);
  const isLong = message.split("\n").length > 5 || message.length > 300;
  return (
    <div>
      <span className={cn("whitespace-pre-wrap break-all", !expanded && isLong && "line-clamp-5")}>
        {message}
      </span>
      {isLong && (
        <button
          type='button'
          onClick={() => setExpanded((v) => !v)}
          className='mt-1 block text-muted-foreground underline hover:text-foreground'
        >
          {expanded ? "Show less ↑" : "Show full error ↓"}
        </button>
      )}
    </div>
  );
};

const CompileResult: React.FC<{ data: CompileOutput }> = ({ data }) => (
  <div className='space-y-1 p-3 font-mono text-xs'>
    <div className='flex items-center gap-2'>
      <CheckCircle2 className='h-3.5 w-3.5 text-emerald-500' />
      <span>{data.models_compiled} models compiled</span>
    </div>
    {data.errors.map((e) => (
      <div key={e.node_id} className='flex items-start gap-2 text-destructive'>
        <XCircle className='mt-0.5 h-3.5 w-3.5 shrink-0' />
        <span>
          <span className='font-medium'>{e.node_id}</span>: {e.message}
        </span>
      </div>
    ))}
  </div>
);

const isSuccess = (status: string) =>
  status === "SUCCESS" || status === "PASS" || status === "success" || status === "pass";

const statusIcon = (status: string) =>
  isSuccess(status) ? (
    <CheckCircle2 className='h-3.5 w-3.5 text-emerald-500' />
  ) : (
    <XCircle className='h-3.5 w-3.5 text-destructive' />
  );

const NodeRow: React.FC<{ result: NodeRunResult; index: number; total: number }> = ({
  result: r,
  index,
  total
}) => {
  const ok = isSuccess(r.status);
  return (
    <div className={`rounded px-2 py-1 ${ok ? "" : "bg-destructive/5"}`}>
      <div className='flex items-baseline gap-2'>
        <span className='shrink-0 text-muted-foreground'>
          {index + 1} of {total}
        </span>
        <span className='font-medium'>{r.name}</span>
        <span
          className={`ml-auto shrink-0 rounded px-1 py-0.5 font-semibold text-xs ${ok ? "bg-emerald-500/10 text-emerald-500" : "bg-destructive/10 text-destructive"}`}
        >
          {ok ? "OK" : "ERROR"}
        </span>
      </div>
      <div className='mt-0.5 flex gap-3 text-muted-foreground'>
        <span>{r.duration_ms}ms</span>
        {r.rows_affected != null && <span>{r.rows_affected} rows affected</span>}
      </div>
      {r.message && (
        <div
          className={`mt-0.5 whitespace-pre-wrap break-all ${ok ? "text-muted-foreground" : "text-destructive"}`}
        >
          {r.message}
        </div>
      )}
    </div>
  );
};

const StreamRunResult: React.FC<{ streamState: RunStreamState }> = ({ streamState }) => {
  const events: RunStreamEvent[] = streamState.phase !== "idle" ? streamState.events : [];
  const isRunning = streamState.phase === "running";

  const completedResults: NodeRunResult[] = events
    .filter(
      (e): e is Extract<RunStreamEvent, { kind: "node_completed" }> => e.kind === "node_completed"
    )
    .map(({ kind: _kind, ...rest }) => rest as NodeRunResult);

  const runningName = isRunning
    ? (() => {
        const startedIds = new Set(
          events
            .filter(
              (e): e is Extract<RunStreamEvent, { kind: "node_started" }> =>
                e.kind === "node_started"
            )
            .map((e) => e.unique_id)
        );
        const completedIds = new Set(completedResults.map((r) => r.unique_id));
        const pendingId = [...startedIds].find((id) => !completedIds.has(id));
        return pendingId
          ? events.find(
              (e): e is Extract<RunStreamEvent, { kind: "node_started" }> =>
                e.kind === "node_started" && e.unique_id === pendingId
            )?.name
          : undefined;
      })()
    : undefined;

  const doneEvent = events.find(
    (e): e is Extract<RunStreamEvent, { kind: "done" }> => e.kind === "done"
  );
  const errorMsg = streamState.phase === "error" ? streamState.message : undefined;

  const knownTotal = doneEvent ? completedResults.length : undefined;

  return (
    <div className='p-3 font-mono text-xs'>
      <div className='space-y-0.5'>
        {completedResults.map((r, i) => (
          <NodeRow
            key={r.unique_id}
            result={r}
            index={i}
            total={knownTotal ?? completedResults.length}
          />
        ))}
        {runningName && (
          <div className='flex items-center gap-2 px-2 py-1 text-muted-foreground'>
            <Loader2 className='h-3.5 w-3.5 animate-spin' />
            <span>{runningName}</span>
            <span className='ml-auto text-xs opacity-50'>running…</span>
          </div>
        )}
      </div>
      {doneEvent && (
        <div className='mt-2 flex items-center gap-1.5 border-t pt-2 text-muted-foreground'>
          {statusIcon(doneEvent.status)}
          <span>
            Finished in {doneEvent.duration_ms}ms · {completedResults.length} model
            {completedResults.length !== 1 ? "s" : ""}
          </span>
        </div>
      )}
      {errorMsg && (
        <div className='mt-2 flex items-start gap-2 border-t pt-2 text-destructive'>
          <XCircle className='mt-0.5 h-3.5 w-3.5 shrink-0' />
          <ExpandableErrorText message={errorMsg} />
        </div>
      )}
    </div>
  );
};

const RunResult: React.FC<{ data: RunOutput }> = ({ data }) => {
  const total = data.results.length;
  return (
    <div className='p-3 font-mono text-xs'>
      <div className='space-y-0.5'>
        {data.results.map((r, i) => (
          <NodeRow key={r.unique_id} result={r} index={i} total={total} />
        ))}
      </div>
      <div className='mt-2 flex items-center gap-1.5 border-t pt-2 text-muted-foreground'>
        {statusIcon(data.status)}
        <span>
          Finished in {data.duration_ms}ms · {total} model{total !== 1 ? "s" : ""}
        </span>
      </div>
    </div>
  );
};

const SeedResult: React.FC<{ data: SeedOutput }> = ({ data }) => {
  const total = data.results.length;
  return (
    <div className='p-3 font-mono text-xs'>
      <div className='space-y-0.5'>
        {data.results.map((r, i) => (
          <NodeRow key={r.unique_id} result={r} index={i} total={total} />
        ))}
      </div>
      <div className='mt-2 flex items-center gap-1.5 border-t pt-2 text-muted-foreground'>
        <CheckCircle2 className='h-3.5 w-3.5 text-emerald-500' />
        <span>
          {data.seeds_loaded} seed{data.seeds_loaded !== 1 ? "s" : ""} loaded
        </span>
      </div>
    </div>
  );
};

const TestResult: React.FC<{ data: TestOutput }> = ({ data }) => {
  const sorted = [...data.results].sort((a, b) => {
    const aFail = !isSuccess(a.status);
    const bFail = !isSuccess(b.status);
    return aFail === bFail ? 0 : aFail ? -1 : 1;
  });

  return (
    <div className='p-3 font-mono text-xs'>
      <div className='mb-3 flex items-center gap-3'>
        {data.passed > 0 && (
          <span className='flex items-center gap-1 text-emerald-500'>
            <CheckCircle2 className='h-3.5 w-3.5' />
            {data.passed} passed
          </span>
        )}
        {data.failed > 0 && (
          <span className='flex items-center gap-1 text-destructive'>
            <XCircle className='h-3.5 w-3.5' />
            {data.failed} failed
          </span>
        )}
        <span className='text-muted-foreground'>{data.tests_run} total</span>
      </div>
      <div className='space-y-1'>
        {sorted.map((r) => (
          <div
            key={r.test_name}
            className={`flex flex-col gap-0.5 rounded px-2 py-1.5 ${isSuccess(r.status) ? "" : "bg-destructive/5"}`}
          >
            <div className='flex items-center gap-2'>
              {statusIcon(r.status)}
              <span className='font-medium'>{r.test_name}</span>
              <span className='ml-auto text-muted-foreground'>{r.duration_ms}ms</span>
            </div>
            <div className='ml-5 flex flex-wrap items-center gap-2 text-muted-foreground'>
              {r.model_name && <span>{r.model_name}</span>}
              {r.column_name && (
                <span className='rounded bg-muted px-1 py-0.5 font-mono text-xs'>
                  {r.column_name}
                </span>
              )}
              {r.failures > 0 && (
                <span className='text-destructive'>
                  {r.failures} failure{r.failures !== 1 ? "s" : ""}
                </span>
              )}
              {r.message && (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <span className='cursor-default truncate'>{r.message}</span>
                  </TooltipTrigger>
                  <TooltipContent side='bottom' className='max-w-sm font-mono text-xs'>
                    {r.message}
                  </TooltipContent>
                </Tooltip>
              )}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};

const AnalyzeResult: React.FC<{ data: AnalyzeOutput }> = ({ data }) => (
  <div className='space-y-1 p-3 font-mono text-xs'>
    <div className='mb-2 text-muted-foreground'>
      {data.models_analyzed} models analyzed · {data.schemas.length} schemas
    </div>
    {data.diagnostics.map((d) => (
      <div key={`${d.kind}:${d.message}`} className='flex items-start gap-2 text-muted-foreground'>
        <XCircle className='mt-0.5 h-3.5 w-3.5 shrink-0 text-destructive' />
        <span>
          <span className='font-medium'>{d.kind}</span>: {d.message}
        </span>
      </div>
    ))}
    {data.contract_violations.map((v) => (
      <div key={`${v.model}:${v.message}`} className='flex items-start gap-2 text-destructive'>
        <XCircle className='mt-0.5 h-3.5 w-3.5 shrink-0' />
        <span>
          <span className='font-medium'>{v.model}</span>: {v.message}
        </span>
      </div>
    ))}
    {data.diagnostics.length === 0 && data.contract_violations.length === 0 && (
      <div className='flex items-center gap-2'>
        <CheckCircle2 className='h-3.5 w-3.5 text-emerald-500' />
        <span>No issues found</span>
      </div>
    )}
  </div>
);

const ErrorResult: React.FC<{ message: string }> = ({ message }) => (
  <div className='p-3 font-mono text-xs'>
    <div className='flex items-start gap-2 text-destructive'>
      <XCircle className='mt-0.5 h-3.5 w-3.5 shrink-0' />
      <ExpandableErrorText message={message} />
    </div>
  </div>
);

const OutputPanel: React.FC<OutputPanelProps> = ({ output, isPending, runStream }) => {
  if (runStream && runStream.phase !== "idle") {
    return <StreamRunResult streamState={runStream} />;
  }

  if (isPending) {
    return (
      <div className='animate-pulse p-3 font-mono text-muted-foreground text-xs'>Running…</div>
    );
  }

  if (!output) {
    return (
      <div className='p-3 text-muted-foreground text-xs'>
        Run Compile, Run, or Test to see output here.
      </div>
    );
  }

  if (output.kind === "compile") return <CompileResult data={output.data} />;
  if (output.kind === "run") return <RunResult data={output.data} />;
  if (output.kind === "seed") return <SeedResult data={output.data} />;
  if (output.kind === "test") return <TestResult data={output.data} />;
  if (output.kind === "analyze") return <AnalyzeResult data={output.data} />;
  if (output.kind === "error") return <ErrorResult message={output.message} />;

  return null;
};

export type { OutputState };
export default OutputPanel;
