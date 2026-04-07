import { useQuery, useQueryClient } from "@tanstack/react-query";
import type { ReactNode, RefObject } from "react";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import BuilderMessageInput from "@/components/BuilderMessageInput";
import ThinkingModeMenu from "@/components/Chat/ChatPanel/ThinkingModeMenu";
import Markdown from "@/components/Markdown";
import MessageInput from "@/components/MessageInput";
import UserMessage from "@/components/Messages/UserMessage";
import ErrorAlert from "@/components/ui/ErrorAlert";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import { Spinner } from "@/components/ui/shadcn/spinner";
import type { SelectableItem } from "@/hooks/analyticsSteps";
import queryKeys from "@/hooks/api/queryKey";
import type { AnalyticsDisplayBlock, SseEvent } from "@/hooks/useAnalyticsRun";
import {
  extractAnswer,
  extractDisplayBlocks,
  sseEventToUiBlock,
  uiBlockToSseEvent,
  useAnalyticsRun
} from "@/hooks/useAnalyticsRun";
import type { BuilderProposedChange } from "@/hooks/useBuilderActivity";
import { extractProposedChangeMetadata, useBuilderActivity } from "@/hooks/useBuilderActivity";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type {
  AnalyticsRunSummary,
  ProposedChangeBlock,
  ThinkingMode,
  UiBlock
} from "@/services/api/analytics";
import { AnalyticsService } from "@/services/api/analytics";
import { consumePendingThinkingMode } from "@/stores/analyticsThinkingMode";
import type { DataContainer, Display } from "@/types/app";
import type { ThreadItem } from "@/types/chat";
import ProcedureRunDagPanel from "../agentic/ProcedureRunDagPanel";
import AcceptedChangePills from "./AcceptedChangePills";
import AnalyticsArtifactSidebar from "./AnalyticsArtifactSidebar";
import AnalyticsReasoningTrace from "./AnalyticsReasoningTrace";
import BuilderActivityPanel from "./BuilderActivityPanel";
import FilePreviewPanel from "./FilePreviewPanel";
import Header from "./Header";
import { parseProposeChange } from "./ProposeChangeDiff";
import SuspensionPrompt from "./SuspensionPrompt";

/** The fixed key used as the data reference inside agentic Display configs. */
const AGENTIC_DATA_KEY = "__agentic_result__";

/** Answer text the backend interprets as an approval for proposed changes. */
const ACCEPT_ANSWER = "Accept";

/**
 * Convert an AnalyticsDisplayBlock into a (Display, DataContainer) pair
 * compatible with the existing <DisplayBlock> component.
 *
 * The inline columns+rows are converted to row-oriented JSON objects and
 * embedded as `TableData.json` under AGENTIC_DATA_KEY, matching the format
 * expected by registerFromTableData → DuckDB WASM.
 */
function toDisplayProps(
  block: AnalyticsDisplayBlock,
  index: number,
  runId: string
): { display: Display; data: DataContainer } {
  const { config, columns, rows } = block;

  // Row-oriented JSON: [{col1: val1, col2: val2}, ...]
  const json = JSON.stringify(
    rows.map((row) => Object.fromEntries(columns.map((col, i) => [col, row[i]])))
  );
  const dataKey = `${AGENTIC_DATA_KEY}_${runId}_${index}`;
  const data: DataContainer = { [dataKey]: { file_path: dataKey, json } };

  let display: Display;
  const ct = config.chart_type;
  if (ct === "line_chart") {
    display = {
      type: "line_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: dataKey,
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
      data: dataKey,
      series: config.series,
      title: config.title
    };
  } else if (ct === "pie_chart") {
    display = {
      type: "pie_chart",
      name: config.name ?? columns[0] ?? "",
      value: config.value ?? columns[1] ?? "",
      data: dataKey,
      title: config.title
    };
  } else {
    // table or unknown — fall back to table
    display = { type: "table", data: dataKey, title: config.title };
  }

  return { display, data };
}

/** Stable wrapper so parent re-renders don't recreate display/data objects. */
const AnalyticsDisplayBlockItem = memo(
  ({ block, index, runId }: { block: AnalyticsDisplayBlock; index: number; runId: string }) => {
    const { display, data } = toDisplayProps(block, index, runId);
    return <DisplayBlock display={display} data={data} />;
  }
);

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

  /** Reset scroll tracking so the next render auto-scrolls to bottom. */
  const scrollToBottom = useCallback(() => {
    isUserScrolledUp.current = false;
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [bottomRef]);

  return { scrollToBottom };
}

// ── Shared run layout ──────────────────────────────────────────────────────────

interface RunEntryProps {
  question: string;
  events: UiBlock[];
  isRunning: boolean;
  isBuilder?: boolean;
  onSelectArtifact: (item: SelectableItem) => void;
  acceptedChanges?: BuilderProposedChange[];
  onSelectChange?: (change: BuilderProposedChange) => void;
  selectedChangeId?: string;
  children?: ReactNode;
}

const RunEntry = ({
  question,
  events,
  isRunning,
  isBuilder,
  onSelectArtifact,
  acceptedChanges,
  onSelectChange,
  selectedChangeId,
  children
}: RunEntryProps) => (
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
    {isBuilder && acceptedChanges && acceptedChanges.length > 0 && onSelectChange && (
      <AcceptedChangePills
        changes={acceptedChanges}
        onSelect={onSelectChange}
        selectedId={selectedChangeId}
      />
    )}
  </div>
);

// ── Completed run (rendered from REST data) ───────────────────────────────────

const PastRunEntry = ({
  run,
  onSelectArtifact,
  onSelectChange,
  selectedChangeId,
  capturedChanges
}: {
  run: AnalyticsRunSummary;
  onSelectArtifact: (
    item: SelectableItem,
    blocks: AnalyticsDisplayBlock[],
    runEvents: SseEvent[]
  ) => void;
  onSelectChange?: (change: BuilderProposedChange) => void;
  selectedChangeId?: string;
  capturedChanges?: BuilderProposedChange[];
}) => {
  const isBuilder = run.agent_id === "__builder__";
  const runSseEvents = useMemo(() => (run.ui_events ?? []).map(uiBlockToSseEvent), [run.ui_events]);
  const runBlocks = useMemo(() => extractDisplayBlocks(runSseEvents), [runSseEvents]);
  const runAnswer = useMemo(
    () => run.answer ?? extractAnswer(runSseEvents),
    [run.answer, runSseEvents]
  );
  // Use changes captured at run-end when available (current session); fall back
  // to server ui_events so pills survive a page reload.
  const acceptedChanges = useMemo((): BuilderProposedChange[] => {
    if (capturedChanges) return capturedChanges;
    if (!isBuilder || run.status !== "done") return [];
    const events = run.ui_events ?? [];
    let counter = 0;
    return events
      .filter((ev): ev is ProposedChangeBlock => ev.event_type === "proposed_change")
      .map((ev) => {
        const { oldContent, isDeletion } = extractProposedChangeMetadata(events, ev.seq);
        return {
          kind: "proposed_change" as const,
          id: `past-${run.run_id}-change-${counter++}`,
          filePath: ev.payload.file_path,
          description: ev.payload.description,
          newContent: ev.payload.new_content,
          oldContent,
          isDeletion: ev.payload.delete ?? isDeletion,
          status: "accepted" as const
        };
      });
  }, [capturedChanges, isBuilder, run.run_id, run.status, run.ui_events]);

  return (
    <RunEntry
      question={run.question}
      events={run.ui_events ?? []}
      isRunning={false}
      isBuilder={isBuilder}
      onSelectArtifact={(item) => onSelectArtifact(item, runBlocks, runSseEvents)}
      acceptedChanges={acceptedChanges}
      onSelectChange={onSelectChange}
      selectedChangeId={selectedChangeId}
    >
      {run.status === "done" && (
        <>
          {run.ui_events &&
            extractDisplayBlocks(run.ui_events.map((e) => uiBlockToSseEvent(e))).map((block, i) => {
              const key = `${block.config.chart_type}-${block.config.title ?? i}`;
              return (
                <AnalyticsDisplayBlockItem key={key} block={block} index={i} runId={run.run_id} />
              );
            })}
          {runAnswer && (
            <div className='rounded-lg border border-border bg-card p-4'>
              <Markdown>{runAnswer}</Markdown>
            </div>
          )}
        </>
      )}
      {run.status === "failed" && (
        <ErrorAlert title='Run failed'>
          {run.error_message && <Markdown>{run.error_message}</Markdown>}
        </ErrorAlert>
      )}
    </RunEntry>
  );
};

// ── Thread ────────────────────────────────────────────────────────────────────

const AnalyticsThread = ({ thread }: Props) => {
  const { project, branchName } = useCurrentProjectBranch();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [followUpQuestion, setFollowUpQuestion] = useState("");
  const [selectedArtifact, setSelectedArtifact] = useState<SelectableItem | null>(null);
  const [selectedRunEvents, setSelectedRunEvents] = useState<SseEvent[]>([]);
  const [activeQuestion, setActiveQuestion] = useState<string | null>(null);
  const [builderPanelOpen, setBuilderPanelOpen] = useState(false);
  const [changeDecisions, setChangeDecisions] = useState<Map<number, "accepted" | "rejected">>(
    () => new Map()
  );
  const [selectedFileChange, setSelectedFileChange] = useState<BuilderProposedChange | null>(null);
  const [selectedDisplayBlocks, setSelectedDisplayBlocks] = useState<AnalyticsDisplayBlock[]>([]);
  // Accepted changes captured per-run when a run reaches terminal state, so they
  // survive the live→PastRunEntry transition (streamingEvents clears on reset).
  const [capturedRunChanges, setCapturedRunChanges] = useState<
    Map<string, BuilderProposedChange[]>
  >(() => new Map());
  const [showProcedurePanel, setShowProcedurePanel] = useState(false);
  const [autoApprove, setAutoApprove] = useState(
    () => localStorage.getItem("builder_auto_approve") === "true"
  );
  const [thinkingMode, setThinkingMode] = useState<ThinkingMode>(
    () => consumePendingThinkingMode(thread.id) ?? "auto"
  );

  const handleAutoApproveChange = useCallback((value: boolean) => {
    setAutoApprove(value);
    localStorage.setItem("builder_auto_approve", String(value));
  }, []);

  const hasSyncedThinkingMode = useRef(false);

  const { scrollToBottom } = useScrollToBottom(containerRef, bottomRef);

  const queryClient = useQueryClient();
  const { state, start, reconnect, answer, stop, reset, isStarting, isAnswering } = useAnalyticsRun(
    { projectId: project.id }
  );
  // Keep a stable ref so effects that only run on isTerminal can read the
  // current events without listing state as a reactive dependency.
  const stateRef = useRef(state);
  stateRef.current = state;
  // Track latest accepted changes so the isTerminal effect can capture them
  // before streamingEvents is cleared by reset().
  const liveAcceptedChangesRef = useRef<BuilderProposedChange[]>([]);

  const {
    data: allRuns = [],
    isLoading: isLookingUp,
    isFetching: isFetchingRuns
  } = useQuery({
    queryKey: queryKeys.analytics.runsByThread(project.id, thread.id),
    queryFn: () => AnalyticsService.getRunsByThread(project.id, thread.id)
  });

  const latestRun = allRuns.at(-1) ?? null;

  const handleThinkingModeChange = useCallback(
    (mode: ThinkingMode) => {
      setThinkingMode(mode);
      const runId = latestRun?.run_id;
      if (runId) {
        AnalyticsService.updateThinkingMode(project.id, runId, mode).catch(() => {});
      }
    },
    [latestRun, project.id]
  );

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
    // Capture accepted changes before streamingEvents clears on reset(), so
    // PastRunEntry can still show pills after the live→history transition.
    if ("runId" in s && s.runId) {
      const accepted = liveAcceptedChangesRef.current;
      if (accepted.length > 0) {
        setCapturedRunChanges((prev) => new Map(prev).set(s.runId, accepted));
      }
    }
    queryClient.invalidateQueries({
      queryKey: queryKeys.analytics.runsByThread(project.id, thread.id)
    });
    // When the builder has accepted changes, selectively invalidate queries
    // based on which file types were actually modified.
    if (liveAcceptedChangesRef.current.length > 0) {
      const paths = liveAcceptedChangesRef.current.map((c) => c.filePath);
      const hasAgent = paths.some((p) => p.endsWith(".agent.yml"));
      const hasWorkflow = paths.some(
        (p) => p.endsWith(".workflow.yml") || p.endsWith(".procedure.yml")
      );
      const hasApp = paths.some((p) => p.endsWith(".app.yml"));

      // Files are always invalidated — any accepted change writes to disk.
      queryClient.invalidateQueries({ queryKey: queryKeys.file.all(project.id, branchName) });
      if (hasAgent) {
        queryClient.invalidateQueries({ queryKey: queryKeys.agent.list(project.id, branchName) });
      }
      if (hasWorkflow) {
        queryClient.invalidateQueries({
          queryKey: queryKeys.workflow.list(project.id, branchName)
        });
      }
      if (hasApp) {
        // Eagerly refetch display structure so the preview reflects layout changes
        // immediately. Data queries are intentionally left alone — they're expensive
        // and should only re-run on explicit refresh or window focus.
        for (const sub of ["list", "getDisplays"] as const) {
          queryClient.refetchQueries({
            queryKey: [...queryKeys.app.all, sub, project.id, branchName],
            type: "all"
          });
        }
      }
      // Auto-open the preview for the last accepted change (skip deletions — file no longer exists).
      const lastChange = [...liveAcceptedChangesRef.current].reverse().find((c) => !c.isDeletion);
      if (lastChange) {
        setSelectedFileChange(lastChange);
        setSelectedArtifact(null);
        setBuilderPanelOpen(false);
      }
    }
  }, [isTerminal, queryClient, project.id, branchName, thread.id]);

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

  // Reset run state and sidebar selection when navigating to a different thread.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on thread change
  useEffect(() => {
    reset();
    setSelectedArtifact(null);
    setSelectedRunEvents([]);
    setShowProcedurePanel(false);
    hasSyncedThinkingMode.current = false;
  }, [thread.id]);

  // Restore the thinking mode from the most recent run once the run list loads.
  // Only syncs once per thread so the user's in-session selection isn't overridden.
  useEffect(() => {
    if (hasSyncedThinkingMode.current || isLookingUp || isFetchingRuns) return;
    hasSyncedThinkingMode.current = true;
    if (latestRun?.thinking_mode) {
      setThinkingMode(latestRun.thinking_mode);
    }
  }, [latestRun, isLookingUp, isFetchingRuns]);

  // ── Derived state ──────────────────────────────────────────────────────────

  const agentId = thread.source;
  const question = thread.input;
  const isBuilder = agentId === "__builder__";

  const isStreaming = state.tag === "running" || state.tag === "suspended";
  const runExists = isStreaming || isTerminal;

  // Auto-open the builder panel only when a change is proposed (agent suspended).
  // Also dismiss any open file preview so the suspension prompt takes focus.
  useEffect(() => {
    if (isBuilder && state.tag === "suspended") {
      setBuilderPanelOpen(true);
      setSelectedFileChange(null);
    }
  }, [isBuilder, state.tag]);
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

  // Guard against stale-cache duplicates: when React Query returns a cached []
  // while a background refetch is in progress (isFetchingRuns=true), we must wait
  // for the refetch to complete before concluding this is truly a first visit.
  // Without this, navigating back to a thread whose run hasn't finished yet would
  // see allRuns=[] + isLoading=false and fire a second auto-start run.
  const isFirstVisit =
    !isLookingUp && !isFetchingRuns && allRuns.length === 0 && state.tag === "idle";

  // Auto-start the run on first visit so the user doesn't need to click a button
  // after already submitting their question from ChatPanel.
  useEffect(() => {
    if (isFirstVisit) {
      start(agentId, question, thread.id, thinkingMode);
    }
  }, [isFirstVisit, agentId, question, thread.id, start, thinkingMode]);

  // For new starts / follow-ups use the locally-tracked question so the UI is responsive
  // before allRuns has picked up the new run. Fall back to latestRun for reconnects.
  const currentQuestion = (runExists ? activeQuestion : null) ?? latestRun?.question ?? question;

  // Builder activity derived from the live event stream.
  const builderActivityItems = useBuilderActivity(streamingEvents, changeDecisions);

  const handleStart = (q: string) => {
    setActiveQuestion(q);
    setChangeDecisions(new Map());
    scrollToBottom();
    start(agentId, q, thread.id, thinkingMode);
  };

  const handleSend = () => {
    const q = followUpQuestion.trim();
    if (!q) return;
    setFollowUpQuestion("");
    handleStart(q);
  };

  // Wrap answer to record accept/reject decisions for the builder activity panel.
  const handleAnswer = useCallback(
    (text: string) => {
      if (isBuilder && state.tag === "suspended") {
        // Find the proposed_change event paired with the current suspension.
        // streamingEvents are UiBlock[] with event_type/seq fields.
        const proposedChange = [...streamingEvents]
          .reverse()
          .find((ev) => ev.event_type === "proposed_change");
        if (proposedChange) {
          const decision = text.toLowerCase().includes("accept") ? "accepted" : "rejected";
          setChangeDecisions((prev) => new Map(prev).set(proposedChange.seq, decision));
          if (decision === "accepted") {
            setBuilderPanelOpen(false);
          }
        }
      }
      answer(text);
    },
    [isBuilder, state.tag, streamingEvents, answer]
  );

  // Auto-approve proposed changes when the toggle is enabled.
  useEffect(() => {
    if (!autoApprove || state.tag !== "suspended") return;
    if (state.questions.length === 1 && parseProposeChange(state.questions[0].prompt)) {
      handleAnswer(ACCEPT_ANSWER);
    }
  }, [autoApprove, state, handleAnswer]);

  const handleSelectFileChange = useCallback((change: BuilderProposedChange) => {
    setSelectedFileChange((prev) => (prev?.id === change.id ? null : change));
    setSelectedArtifact(null);
    setBuilderPanelOpen(false);
  }, []);

  const liveAcceptedChanges = useMemo(
    () =>
      builderActivityItems.filter(
        (i): i is BuilderProposedChange => i.kind === "proposed_change" && i.status === "accepted"
      ),
    [builderActivityItems]
  );
  // Keep ref in sync so the isTerminal effect can read current value without a dep.
  liveAcceptedChangesRef.current = liveAcceptedChanges;

  const handleSelectArtifact = useCallback(
    (item: SelectableItem, blocks: AnalyticsDisplayBlock[] = [], runEvents: SseEvent[] = []) => {
      if (item.kind === "procedure") {
        setShowProcedurePanel((prev) => !prev);
        return;
      }
      setSelectedArtifact(item);
      setSelectedDisplayBlocks(blocks);
      setSelectedFileChange(null);
      setBuilderPanelOpen(false);
      if (runEvents.length > 0) setSelectedRunEvents(runEvents);
    },
    []
  );

  return (
    <div className='flex h-full flex-col'>
      <Header thread={thread} />

      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        <ResizablePanel
          defaultSize={
            isBuilder && builderPanelOpen
              ? 50
              : selectedFileChange
                ? 50
                : selectedArtifact || (showProcedurePanel && procedureInfo)
                  ? 50
                  : 100
          }
          minSize={30}
        >
          <div className='flex h-full w-full flex-1 flex-col py-4'>
            <div
              ref={containerRef}
              className='flex w-full flex-1 flex-col overflow-y-auto [scrollbar-gutter:stable_both-edges]'
            >
              <div className='mx-auto mb-6 w-full max-w-page-content px-4'>
                {(isLookingUp || (isFetchingRuns && allRuns.length === 0)) && (
                  <div className='flex items-center gap-2 text-muted-foreground text-sm'>
                    <Spinner className='size-3' />
                  </div>
                )}

                {historyRuns.map((run) => (
                  <PastRunEntry
                    key={run.run_id}
                    run={run}
                    onSelectArtifact={handleSelectArtifact}
                    onSelectChange={handleSelectFileChange}
                    selectedChangeId={selectedFileChange?.id}
                    capturedChanges={capturedRunChanges.get(run.run_id)}
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
                    isBuilder={isBuilder}
                    onSelectArtifact={(item) =>
                      handleSelectArtifact(item, state.tag === "done" ? state.displayBlocks : [])
                    }
                    acceptedChanges={liveAcceptedChanges}
                    onSelectChange={handleSelectFileChange}
                    selectedChangeId={selectedFileChange?.id}
                  >
                    {state.tag === "done" && (
                      <div className='flex flex-col gap-4'>
                        {state.displayBlocks.map((block, i) => {
                          const key = `${block.config.chart_type}-${block.config.title ?? i}`;
                          return (
                            <AnalyticsDisplayBlockItem
                              key={key}
                              block={block}
                              index={i}
                              runId={state.runId}
                            />
                          );
                        })}
                        {state.answer && (
                          <div className='rounded-lg border border-border bg-card p-4'>
                            <Markdown>{state.answer}</Markdown>
                          </div>
                        )}
                      </div>
                    )}

                    {state.tag === "failed" && (
                      <ErrorAlert
                        title={state.message === "Cancelled" ? "Cancelled" : "Run failed"}
                        actions={
                          <Button
                            size='sm'
                            variant='outline'
                            onClick={() => {
                              reset();
                              handleStart(currentQuestion);
                            }}
                          >
                            Retry
                          </Button>
                        }
                      >
                        {state.message !== "Cancelled" && <Markdown>{state.message}</Markdown>}
                      </ErrorAlert>
                    )}
                  </RunEntry>
                )}

                <div ref={bottomRef} />
              </div>
            </div>

            <div className='mx-auto w-full max-w-page-content p-4 pt-0'>
              {state.tag === "suspended" ? (
                <SuspensionPrompt
                  questions={state.questions}
                  onAnswer={handleAnswer}
                  isAnswering={isAnswering}
                />
              ) : isBuilder ? (
                <BuilderMessageInput
                  onSend={handleStart}
                  onStop={stop}
                  disabled={state.tag === "running" || isStarting}
                  isLoading={state.tag === "running" || isStarting}
                  autoApprove={autoApprove}
                  onAutoApproveChange={handleAutoApproveChange}
                />
              ) : (
                <div className='flex-col items-end gap-2 rounded-md border border-border bg-secondary'>
                  <div className='flex-1'>
                    <MessageInput
                      value={followUpQuestion}
                      onChange={setFollowUpQuestion}
                      onSend={handleSend}
                      onStop={stop}
                      disabled={state.tag === "running" || isStarting}
                      isLoading={state.tag === "running" || isStarting}
                      inputClassName='border-0'
                      noFocusRing
                    />
                  </div>
                  <div className='flex items-center justify-end px-2 pb-2'>
                    <ThinkingModeMenu
                      value={thinkingMode}
                      onChange={handleThinkingModeChange}
                      disabled={state.tag === "running" || isStarting}
                    />
                  </div>
                </div>
              )}
            </div>
          </div>
        </ResizablePanel>

        {isBuilder && builderPanelOpen ? (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={50} minSize={20} maxSize={70}>
              <BuilderActivityPanel
                items={builderActivityItems}
                isRunning={isStreaming}
                isSuspended={state.tag === "suspended"}
                onAnswer={handleAnswer}
                isAnswering={isAnswering}
                onClose={() => setBuilderPanelOpen(false)}
              />
            </ResizablePanel>
          </>
        ) : selectedFileChange ? (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={50} minSize={20} maxSize={70}>
              <FilePreviewPanel
                change={selectedFileChange}
                onClose={() => setSelectedFileChange(null)}
              />
            </ResizablePanel>
          </>
        ) : (
          selectedArtifact && (
            <>
              <ResizableHandle withHandle />
              <ResizablePanel defaultSize={50} minSize={20} maxSize={70}>
                <AnalyticsArtifactSidebar
                  item={selectedArtifact}
                  displayBlocks={selectedDisplayBlocks}
                  runEvents={"events" in state ? state.events : selectedRunEvents}
                  isRunning={isStreaming}
                  onClose={() => setSelectedArtifact(null)}
                />
              </ResizablePanel>
            </>
          )
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
