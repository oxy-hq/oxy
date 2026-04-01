import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ReactNode, RefObject } from "react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import Markdown from "@/components/Markdown";
import MessageInput from "@/components/MessageInput";
import UserMessage from "@/components/Messages/UserMessage";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import type { SelectableItem } from "@/hooks/analyticsSteps";
import queryKeys from "@/hooks/api/queryKey";
import type { AnalyticsDisplayBlock, SseEvent } from "@/hooks/useAnalyticsRun";
import {
  extractDisplayBlocks,
  sseEventToUiBlock,
  uiBlockToSseEvent,
  useAnalyticsRun
} from "@/hooks/useAnalyticsRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { AnalyticsRunSummary, UiBlock } from "@/services/api/analytics";
import { AnalyticsService } from "@/services/api/analytics";
import type { DataContainer, Display } from "@/types/app";
import type { ThreadItem } from "@/types/chat";
import ProcedureRunDagPanel from "../agentic/ProcedureRunDagPanel";
import AnalyticsArtifactSidebar from "./AnalyticsArtifactSidebar";
import AnalyticsReasoningTrace from "./AnalyticsReasoningTrace";
import Header from "./Header";
import SuspensionPrompt from "./SuspensionPrompt";

/** The fixed key used as the data reference inside agentic Display configs. */
const AGENTIC_DATA_KEY = "__agentic_result__";

/**
 * Convert an AnalyticsDisplayBlock into a (Display, DataContainer) pair
 * compatible with the existing <DisplayBlock> component.
 *
 * The inline columns+rows are converted to row-oriented JSON objects and
 * embedded as `TableData.json` under AGENTIC_DATA_KEY, matching the format
 * expected by registerFromTableData → DuckDB WASM.
 */
function toDisplayProps(block: AnalyticsDisplayBlock): { display: Display; data: DataContainer } {
  const { config, columns, rows } = block;

  // Row-oriented JSON: [{col1: val1, col2: val2}, ...]
  const json = JSON.stringify(
    rows.map((row) => Object.fromEntries(columns.map((col, i) => [col, row[i]])))
  );
  const data: DataContainer = { [AGENTIC_DATA_KEY]: { file_path: AGENTIC_DATA_KEY, json } };

  let display: Display;
  const ct = config.chart_type;
  if (ct === "line_chart") {
    display = {
      type: "line_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      series: config.series,
      title: config.title,
      xAxisTitle: config.x_axis_label,
      yAxisTitle: config.y_axis_label
    };
  } else if (ct === "bar_chart") {
    display = {
      type: "bar_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      series: config.series,
      title: config.title
    };
  } else if (ct === "pie_chart") {
    display = {
      type: "pie_chart",
      name: config.name ?? columns[0] ?? "",
      value: config.value ?? columns[1] ?? "",
      data: AGENTIC_DATA_KEY,
      title: config.title
    };
  } else {
    // table or unknown — fall back to table
    display = { type: "table", data: AGENTIC_DATA_KEY, title: config.title };
  }

  return { display, data };
}

/** Stable wrapper so parent re-renders don't recreate display/data objects. */
const AnalyticsDisplayBlockItem = memo(({ block }: { block: AnalyticsDisplayBlock }) => {
  const { display, data } = toDisplayProps(block);
  return <DisplayBlock display={display} data={data} />;
});

interface Props {
  thread: ThreadItem;
}

// ── Scroll-to-bottom behavior ─────────────────────────────────────────────────

function useScrollToBottom(
  containerRef: RefObject<HTMLDivElement | null>,
  bottomRef: RefObject<HTMLDivElement | null>
) {
  const isUserScrolledUp = useRef(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: containerRef is a stable ref object — .current cannot be tracked by React
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const onScroll = () => {
      isUserScrolledUp.current =
        container.scrollHeight - container.scrollTop - container.clientHeight > 100;
    };
    container.addEventListener("scroll", onScroll);
    return () => container.removeEventListener("scroll", onScroll);
  }, []);

  // Runs after every render; the scroll guard makes it cheap.
  useEffect(() => {
    if (!isUserScrolledUp.current) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  });
}

// ── Shared run layout ──────────────────────────────────────────────────────────

interface RunEntryProps {
  question: string;
  events: UiBlock[];
  isRunning: boolean;
  onSelectArtifact: (item: SelectableItem) => void;
  children?: ReactNode;
}

const RunEntry = ({ question, events, isRunning, onSelectArtifact, children }: RunEntryProps) => (
  <div className='mb-8'>
    <div className='mb-4 flex justify-end'>
      <UserMessage content={question} />
    </div>
    {(events.length > 0 || isRunning) && (
      <div className='mb-4'>
        <AnalyticsReasoningTrace
          events={events}
          isRunning={isRunning}
          onSelectArtifact={onSelectArtifact}
        />
      </div>
    )}
    {children}
  </div>
);

// ── Completed run (rendered from REST data) ───────────────────────────────────

const PastRunEntry = ({
  run,
  onSelectArtifact
}: {
  run: AnalyticsRunSummary;
  onSelectArtifact: (
    item: SelectableItem,
    blocks: AnalyticsDisplayBlock[],
    runEvents: SseEvent[]
  ) => void;
}) => {
  const runSseEvents = (run.ui_events ?? []).map(uiBlockToSseEvent);
  const runBlocks = extractDisplayBlocks(runSseEvents);
  return (
    <RunEntry
      question={run.question}
      events={run.ui_events ?? []}
      isRunning={false}
      onSelectArtifact={(item) => onSelectArtifact(item, runBlocks, runSseEvents)}
    >
      {run.status === "done" && (
        <>
          {run.ui_events &&
            extractDisplayBlocks(run.ui_events.map((e) => uiBlockToSseEvent(e))).map((block, i) => {
              const key = `${block.config.chart_type}-${block.config.title ?? i}`;
              return <AnalyticsDisplayBlockItem key={key} block={block} />;
            })}
          {run.answer && (
            <div className='rounded-lg border border-border bg-card p-4'>
              <Markdown>{run.answer}</Markdown>
            </div>
          )}
        </>
      )}
      {run.status === "failed" && (
        <div className='rounded-lg border border-destructive bg-destructive/10 p-4'>
          <p className='font-medium text-destructive text-sm'>Run failed</p>
          {run.error_message && <Markdown>{run.error_message}</Markdown>}
        </div>
      )}
    </RunEntry>
  );
};

// ── Thread ────────────────────────────────────────────────────────────────────

const AnalyticsThread = ({ thread }: Props) => {
  const { project } = useCurrentProjectBranch();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const [selectedArtifact, setSelectedArtifact] = useState<SelectableItem | null>(null);
  const [selectedRunEvents, setSelectedRunEvents] = useState<SseEvent[]>([]);
  const [activeQuestion, setActiveQuestion] = useState<string | null>(null);
  const [showProcedurePanel, setShowProcedurePanel] = useState(false);

  useScrollToBottom(containerRef, bottomRef);

  const queryClient = useQueryClient();
  const { state, start, reconnect, answer, stop, reset, isStarting, isAnswering } = useAnalyticsRun(
    { projectId: project.id }
  );
  // Keep a stable ref so effects that only run on isTerminal can read the
  // current events without listing state as a reactive dependency.
  const stateRef = useRef(state);
  stateRef.current = state;

  const { data: allRuns = [], isLoading: isLookingUp } = useQuery({
    queryKey: queryKeys.analytics.runsByThread(project.id, thread.id),
    queryFn: () => AnalyticsService.getRunsByThread(project.id, thread.id)
  });

  const latestRun = allRuns.at(-1) ?? null;

  // Page load: reconnect SSE only for active runs. Terminal runs render via allRuns.
  useEffect(() => {
    if (state.tag !== "idle" || !latestRun) return;
    if (latestRun.status === "running" || latestRun.status === "suspended") {
      reconnect(latestRun.run_id, latestRun.status);
    }
  }, [latestRun, state.tag, reconnect]);

  // When a run reaches a terminal state, invalidate allRuns so the completed run
  // appears with its ui_events on the next render. Also freeze the SSE events so
  // the sidebar keeps its state after reset() clears the run from memory.
  const isTerminal = state.tag === "done" || state.tag === "failed";
  useEffect(() => {
    if (!isTerminal) return;
    const s = stateRef.current;
    if ("events" in s) setSelectedRunEvents(s.events);
    queryClient.invalidateQueries({
      queryKey: queryKeys.analytics.runsByThread(project.id, thread.id)
    });
  }, [isTerminal, queryClient, project.id, thread.id]);

  // Once allRuns reflects the terminal run, reset state to idle so it transitions
  // to a PastRunEntry. Uses the runId string (stable) rather than the full state object.
  const terminalRunId = state.tag === "done" || state.tag === "failed" ? state.runId : null;
  useEffect(() => {
    if (!terminalRunId) return;
    const reflected = allRuns.some(
      (r) => r.run_id === terminalRunId && (r.status === "done" || r.status === "failed")
    );
    if (reflected) reset();
  }, [terminalRunId, allRuns, reset]);

  // Clear the tracked question once the run is idle (terminal → PastRunEntry transition done).
  useEffect(() => {
    if (state.tag === "idle") setActiveQuestion(null);
  }, [state.tag]);

  const autoStartedRef = useRef(false);

  // Reset run state and sidebar selection when navigating to a different thread.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on thread change
  useEffect(() => {
    reset();
    autoStartedRef.current = false;
    setSelectedArtifact(null);
    setSelectedRunEvents([]);
    setShowProcedurePanel(false);
  }, [thread.id]);

  // ── Derived state ──────────────────────────────────────────────────────────

  const agentId = thread.source;
  const question = thread.input;

  const isStreaming = state.tag === "running" || state.tag === "suspended";
  const runExists = isStreaming || isTerminal;

  // Current SSE events: prefer live streaming events, fall back to frozen events from last run.
  const currentEvents = "events" in state ? state.events : selectedRunEvents;

  // Derive procedure info from the last procedure_started event in the current events.
  const procedureInfo = useMemo(() => {
    for (let i = currentEvents.length - 1; i >= 0; i--) {
      const ev = currentEvents[i];
      if (ev.type === "procedure_started") {
        const data = ev.data as {
          procedure_name: string;
          steps: Array<{ name: string; task_type: string }>;
        };
        return { procedureName: data.procedure_name, steps: data.steps };
      }
    }
    return null;
  }, [currentEvents]);

  // Exclude the active run from history while it is being streamed / transitioning to
  // PastRunEntry to avoid rendering it twice (once live, once via allRuns).
  const activeRunId = state.tag !== "idle" && "runId" in state && state.runId ? state.runId : null;
  const historyRuns = activeRunId ? allRuns.filter((r) => r.run_id !== activeRunId) : allRuns;

  const streamingEvents = runExists ? state.events.map(sseEventToUiBlock) : ([] as UiBlock[]);

  const isFirstVisit = !isLookingUp && allRuns.length === 0 && state.tag === "idle";

  // Auto-start the run on first visit so the user doesn't need to click a button
  // after already submitting their question from ChatPanel.
  useEffect(() => {
    if (isFirstVisit && !autoStartedRef.current) {
      autoStartedRef.current = true;
      start(agentId, question, thread.id);
    }
  }, [isFirstVisit, agentId, question, thread.id, start]);

  // For new starts / follow-ups use the locally-tracked question so the UI is responsive
  // before allRuns has picked up the new run. Fall back to latestRun for reconnects.
  const currentQuestion = (runExists ? activeQuestion : null) ?? latestRun?.question ?? question;

  const handleStart = (q: string) => {
    setActiveQuestion(q);
    start(agentId, q, thread.id);
  };

  const handleSend = () => {
    const q = followUpQuestion.trim();
    if (!q) return;
    setFollowUpQuestion("");
    handleStart(q);
  };

  const handleSelectArtifact = useCallback(
    (item: SelectableItem, _blocks: AnalyticsDisplayBlock[] = [], runEvents: SseEvent[] = []) => {
      if (item.kind === "procedure") {
        setShowProcedurePanel((prev) => !prev);
        return;
      }
      setSelectedArtifact(item);
      if (runEvents.length > 0) setSelectedRunEvents(runEvents);
    },
    []
  );

  return (
    <div className='flex h-full flex-col'>
      <Header thread={thread} />

      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        <ResizablePanel
          defaultSize={selectedArtifact || (showProcedurePanel && procedureInfo) ? 55 : 100}
          minSize={30}
        >
          <div className='flex h-full w-full flex-1 flex-col py-4'>
            <div
              ref={containerRef}
              className='customScrollbar flex w-full flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable_both-edges]'
            >
              <div className='mx-auto mb-6 w-full max-w-page-content px-4'>
                {isLookingUp && (
                  <div className='flex items-center gap-2 text-muted-foreground text-sm'>
                    <span className='h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent' />
                    Loading…
                  </div>
                )}

                {historyRuns.map((run) => (
                  <PastRunEntry
                    key={run.run_id}
                    run={run}
                    onSelectArtifact={handleSelectArtifact}
                  />
                ))}

                {isFirstVisit && (
                  <RunEntry
                    question={question}
                    events={[]}
                    isRunning={true}
                    onSelectArtifact={handleSelectArtifact}
                  />
                )}

                {runExists && (
                  <RunEntry
                    question={currentQuestion}
                    events={streamingEvents}
                    isRunning={isStreaming}
                    onSelectArtifact={(item) =>
                      handleSelectArtifact(item, state.tag === "done" ? state.displayBlocks : [])
                    }
                  >
                    {state.tag === "done" && (
                      <div className='flex flex-col gap-4'>
                        {state.displayBlocks.map((block, i) => {
                          const key = `${block.config.chart_type}-${block.config.title ?? i}`;
                          return <AnalyticsDisplayBlockItem key={key} block={block} />;
                        })}
                        {state.answer && (
                          <div className='rounded-lg border border-border bg-card p-4'>
                            <Markdown>{state.answer}</Markdown>
                          </div>
                        )}
                      </div>
                    )}

                    {state.tag === "failed" && (
                      <div className='rounded-lg border border-destructive bg-destructive/10 p-4'>
                        <p className='font-medium text-destructive text-sm'>Run failed</p>
                        <Markdown>{state.message}</Markdown>
                        <button
                          type='button'
                          onClick={() => {
                            reset();
                            handleStart(currentQuestion);
                          }}
                          className='mt-3 text-sm underline'
                        >
                          Retry
                        </button>
                      </div>
                    )}
                  </RunEntry>
                )}

                <div ref={bottomRef} />
              </div>
            </div>

            <div className='mx-auto w-full max-w-page-content p-6 pt-0'>
              {state.tag === "suspended" ? (
                <SuspensionPrompt
                  questions={state.questions}
                  onAnswer={answer}
                  isAnswering={isAnswering}
                />
              ) : (
                <MessageInput
                  value={followUpQuestion}
                  onChange={setFollowUpQuestion}
                  onSend={handleSend}
                  onStop={stop}
                  disabled={state.tag === "running" || isStarting}
                  isLoading={state.tag === "running" || isStarting}
                />
              )}
            </div>
          </div>
        </ResizablePanel>

        {selectedArtifact && (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={40} minSize={20} maxSize={70}>
              <AnalyticsArtifactSidebar
                item={selectedArtifact}
                displayBlocks={extractDisplayBlocks(currentEvents)}
                runEvents={currentEvents}
                isRunning={isStreaming}
                onClose={() => setSelectedArtifact(null)}
              />
            </ResizablePanel>
          </>
        )}

        {showProcedurePanel && procedureInfo && (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={40} minSize={20} maxSize={70}>
              <ProcedureRunDagPanel
                procedureName={procedureInfo.procedureName}
                steps={procedureInfo.steps}
                events={currentEvents}
                isRunning={isStreaming}
                onClose={() => setShowProcedurePanel(false)}
              />
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>
    </div>
  );
};

export default AnalyticsThread;
