import { useQuery, useQueryClient } from "@tanstack/react-query";
import { LayoutDashboard } from "lucide-react";
import type { ReactNode, RefObject } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import Markdown from "@/components/Markdown";
import MessageInput from "@/components/MessageInput";
import UserMessage from "@/components/Messages/UserMessage";
import { Button } from "@/components/ui/shadcn/button";
import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup
} from "@/components/ui/shadcn/resizable";
import queryKeys from "@/hooks/api/queryKey";
import { sseEventToUiBlock, useAppBuilderRun } from "@/hooks/useAppBuilderRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import type { UiBlock } from "@/services/api/analytics";
import type { AppBuilderRunSummary } from "@/services/api/appBuilder";
import { AppBuilderService } from "@/services/api/appBuilder";
import type { ThreadItem } from "@/types/chat";
import SuspensionPrompt from "../analytics/SuspensionPrompt";
import type { AppBuilderSelectableItem } from "./AppBuilderArtifactSidebar";
import AppBuilderArtifactSidebar from "./AppBuilderArtifactSidebar";
import AppBuilderReasoningTrace from "./AppBuilderReasoningTrace";
import Header from "./Header";

/** Extract the interpreting step's summary text from a list of UI events. */
function extractInterpretingSummary(events: UiBlock[]): string | null {
  let inInterpreting = false;
  let summary = "";
  for (const ev of events) {
    if (ev.event_type === "step_start" && ev.payload.label === "interpreting") {
      inInterpreting = true;
      summary = "";
    } else if (ev.event_type === "step_end" && inInterpreting) {
      inInterpreting = false;
    } else if (ev.event_type === "text_delta" && inInterpreting) {
      summary += ev.payload.token ?? "";
    }
  }
  return summary.trim() || null;
}

interface Props {
  thread: ThreadItem;
}

// ── Scroll-to-bottom behavior ─────────────────────────────────────────────────

function useScrollToBottom(
  containerRef: RefObject<HTMLDivElement | null>,
  bottomRef: RefObject<HTMLDivElement | null>
) {
  const isUserScrolledUp = useRef(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: containerRef is a stable ref object
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
  onSelectArtifact: (item: AppBuilderSelectableItem) => void;
  children?: ReactNode;
}

const RunEntry = ({ question, events, isRunning, onSelectArtifact, children }: RunEntryProps) => (
  <div className='mb-8'>
    <div className='mb-4 flex justify-end'>
      <UserMessage content={question} />
    </div>
    {(events.length > 0 || isRunning) && (
      <div className='mb-4'>
        <AppBuilderReasoningTrace
          events={events}
          isRunning={isRunning}
          onSelectArtifact={onSelectArtifact}
        />
      </div>
    )}
    {children}
  </div>
);

// ── Past run entry ────────────────────────────────────────────────────────────

const PastRunEntry = ({
  run,
  onViewApp,
  onSelectArtifact
}: {
  run: AppBuilderRunSummary;
  onViewApp: (runId: string) => void;
  onSelectArtifact: (item: AppBuilderSelectableItem) => void;
}) => {
  return (
    <RunEntry
      question={run.request}
      events={run.ui_events ?? []}
      isRunning={false}
      onSelectArtifact={onSelectArtifact}
    >
      {run.status === "done" && (
        <div className='rounded-lg border border-border bg-card p-4'>
          <Markdown>
            {extractInterpretingSummary(run.ui_events ?? []) ?? "App built successfully."}
          </Markdown>
          <Button
            variant='outline'
            size='sm'
            className='mt-3'
            onClick={() => onViewApp(run.run_id)}
          >
            <LayoutDashboard />
            View App
          </Button>
        </div>
      )}
      {run.status === "failed" && (
        <div className='rounded-lg border border-destructive bg-destructive/10 p-4'>
          <p className='font-medium text-destructive text-sm'>Build failed</p>
          {run.error_message && (
            <p className='mt-1 text-muted-foreground text-sm'>{run.error_message}</p>
          )}
        </div>
      )}
    </RunEntry>
  );
};

// ── Thread ────────────────────────────────────────────────────────────────────

const AppBuilderThread = ({ thread }: Props) => {
  const { project } = useCurrentProjectBranch();
  const bottomRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [followUpRequest, setFollowUpRequest] = useState("");
  const [appPath64, setAppPath64] = useState<string | null>(null);
  const [activeRequest, setActiveRequest] = useState<string | null>(null);
  const [selectedPanel, setSelectedPanel] = useState<AppBuilderSelectableItem | null>(null);
  const userClosedSidebarRef = useRef(false);

  useScrollToBottom(containerRef, bottomRef);

  const queryClient = useQueryClient();
  const { state, start, reconnect, answer, retry, stop, reset, isStarting, isAnswering } =
    useAppBuilderRun({ projectId: project.id });

  const stateRef = useRef(state);
  stateRef.current = state;

  const { data: allRuns = [], isLoading: isLookingUp } = useQuery({
    queryKey: queryKeys.appBuilder.runsByThread(project.id, thread.id),
    queryFn: () => AppBuilderService.getRunsByThread(project.id, thread.id)
  });

  const latestRun = allRuns.at(-1) ?? null;

  // Page load: reconnect SSE for active runs.
  useEffect(() => {
    if (state.tag !== "idle" || !latestRun) return;
    if (latestRun.status === "running" || latestRun.status === "suspended") {
      reconnect(latestRun.run_id);
    }
  }, [latestRun, state.tag, reconnect]);

  // When active run is done, save YAML to disk and open preview sidebar.
  const doneRunId = state.tag === "done" ? state.runId : null;
  useEffect(() => {
    if (!doneRunId || userClosedSidebarRef.current) return;
    AppBuilderService.saveRun(project.id, doneRunId)
      .then(({ app_path64 }) => {
        setAppPath64(app_path64);
        setSelectedPanel({ kind: "app_preview", appPath64: app_path64 });
      })
      .catch(console.error);
  }, [doneRunId, project.id]);

  // When a run reaches a terminal state, freeze the SSE events for the sidebar
  // and invalidate allRuns so the completed run appears.
  const isTerminal = state.tag === "done" || state.tag === "failed";
  useEffect(() => {
    if (!isTerminal) return;
    queryClient.invalidateQueries({
      queryKey: queryKeys.appBuilder.runsByThread(project.id, thread.id)
    });
  }, [isTerminal, queryClient, project.id, thread.id]);

  // Once allRuns reflects the terminal run, reset state to idle.
  const terminalRunId = state.tag === "done" || state.tag === "failed" ? state.runId : null;
  useEffect(() => {
    if (!terminalRunId) return;
    const reflected = allRuns.some(
      (r) => r.run_id === terminalRunId && (r.status === "done" || r.status === "failed")
    );
    if (reflected) reset();
  }, [terminalRunId, allRuns, reset]);

  useEffect(() => {
    if (state.tag === "idle") setActiveRequest(null);
  }, [state.tag]);

  const autoStartedRef = useRef(false);

  // Reset on thread change.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on thread change
  useEffect(() => {
    reset();
    autoStartedRef.current = false;
    setAppPath64(null);
    setSelectedPanel(null);
    userClosedSidebarRef.current = false;
  }, [thread.id]);

  // ── Derived state ──────────────────────────────────────────────────────────

  const agentId = thread.source;
  const request = thread.input;

  const isStreaming = state.tag === "running" || state.tag === "suspended";
  const runExists = isStreaming || isTerminal;

  const activeRunId = state.tag !== "idle" && "runId" in state && state.runId ? state.runId : null;
  const historyRuns = activeRunId ? allRuns.filter((r) => r.run_id !== activeRunId) : allRuns;

  const streamingEvents = runExists ? state.events.map(sseEventToUiBlock) : [];

  const isFirstVisit = !isLookingUp && allRuns.length === 0 && state.tag === "idle";

  // Auto-start the run on first visit so the user doesn't need to click a button
  // after already submitting their question from ChatPanel.
  useEffect(() => {
    if (isFirstVisit && !autoStartedRef.current) {
      autoStartedRef.current = true;
      start(agentId, request, thread.id);
    }
  }, [isFirstVisit, agentId, request, thread.id, start]);

  const currentRequest = (runExists ? activeRequest : null) ?? latestRun?.request ?? request;

  const handleStart = (req: string) => {
    setActiveRequest(req);
    userClosedSidebarRef.current = false;
    start(agentId, req, thread.id);
  };

  const handleSend = () => {
    const req = followUpRequest.trim();
    if (!req) return;
    setFollowUpRequest("");
    handleStart(req);
  };

  const handleSelectArtifact = useCallback((item: AppBuilderSelectableItem) => {
    setSelectedPanel(item);
  }, []);

  const handleViewApp = useCallback(
    async (runId: string) => {
      try {
        const { app_path64 } = await AppBuilderService.saveRun(project.id, runId);
        userClosedSidebarRef.current = false;
        setAppPath64(app_path64);
        setSelectedPanel({ kind: "app_preview", appPath64: app_path64 });
      } catch (e) {
        console.error("Failed to save app:", e);
      }
    },
    [project.id]
  );

  const handleClosePanel = () => {
    if (selectedPanel?.kind === "app_preview") {
      userClosedSidebarRef.current = true;
    }
    setSelectedPanel(null);
  };

  return (
    <div className='flex h-full flex-col'>
      <Header thread={thread} />

      <ResizablePanelGroup direction='horizontal' className='flex-1'>
        <ResizablePanel defaultSize={selectedPanel ? 55 : 100} minSize={30}>
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
                    onViewApp={handleViewApp}
                    onSelectArtifact={handleSelectArtifact}
                  />
                ))}

                {isFirstVisit && (
                  <RunEntry
                    question={request}
                    events={[]}
                    isRunning={true}
                    onSelectArtifact={handleSelectArtifact}
                  />
                )}

                {runExists && (
                  <RunEntry
                    question={currentRequest}
                    events={streamingEvents}
                    isRunning={isStreaming}
                    onSelectArtifact={(item) => handleSelectArtifact(item)}
                  >
                    {state.tag === "done" && (
                      <div className='rounded-lg border border-border bg-card p-4'>
                        <Markdown>
                          {extractInterpretingSummary(streamingEvents) ?? "App built successfully."}
                        </Markdown>
                        {appPath64 && (
                          <Button
                            variant='outline'
                            size='sm'
                            className='mt-3'
                            onClick={() => {
                              userClosedSidebarRef.current = false;
                              setSelectedPanel({ kind: "app_preview", appPath64 });
                            }}
                          >
                            <LayoutDashboard />
                            View App
                          </Button>
                        )}
                      </div>
                    )}

                    {state.tag === "failed" && (
                      <div className='rounded-lg border border-destructive bg-destructive/10 p-4'>
                        <p className='font-medium text-destructive text-sm'>Build failed</p>
                        <p className='mt-1 text-muted-foreground text-sm'>{state.message}</p>
                        <div className='mt-3 flex gap-3'>
                          <button
                            type='button'
                            onClick={() => retry()}
                            className='text-sm underline'
                          >
                            Retry from checkpoint
                          </button>
                          <button
                            type='button'
                            onClick={() => {
                              reset();
                              handleStart(currentRequest);
                            }}
                            className='text-muted-foreground text-sm underline'
                          >
                            Restart from scratch
                          </button>
                        </div>
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
                  value={followUpRequest}
                  onChange={setFollowUpRequest}
                  onSend={handleSend}
                  onStop={stop}
                  disabled={state.tag === "running" || isStarting}
                  isLoading={state.tag === "running" || isStarting}
                />
              )}
            </div>
          </div>
        </ResizablePanel>

        {selectedPanel && (
          <>
            <ResizableHandle withHandle />
            <ResizablePanel defaultSize={45} minSize={20} maxSize={70}>
              <AppBuilderArtifactSidebar item={selectedPanel} onClose={handleClosePanel} />
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>
    </div>
  );
};

export default AppBuilderThread;
