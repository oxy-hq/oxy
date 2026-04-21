import { cx } from "class-variance-authority";
import { ArrowUp, Loader2 } from "lucide-react";
import { memo, useState } from "react";
import { DisplayBlock } from "@/components/AppPreview/Displays";
import Markdown from "@/components/Markdown";
import EmptyState from "@/components/ui/EmptyState";
import { Button } from "@/components/ui/shadcn/button";
import { Textarea } from "@/components/ui/shadcn/textarea";
import {
  type AnalyticsDisplayBlock,
  sseEventToUiBlock,
  useAnalyticsRun
} from "@/hooks/useAnalyticsRun";
import useCurrentProjectBranch from "@/hooks/useCurrentProjectBranch";
import { useEnterSubmit } from "@/hooks/useEnterSubmit";
import { decodeBase64 } from "@/libs/encoding";
import AnalyticsReasoningTrace from "@/pages/thread/analytics/AnalyticsReasoningTrace";
import SuspensionPrompt from "@/pages/thread/analytics/SuspensionPrompt";
import type { UiBlock } from "@/services/api/analytics";
import { getThreadIdFromPath } from "@/stores/useAgentThread";

/**
 * Returns the agent_id for the analytics API from a file path.
 * The backend resolves the config as `project_path.join(agent_id)`, so agent_id
 * must be the path relative to the project root (e.g. "analytics.agentic.yml").
 */
export const getAgentIdFromPath = (filePath: string): string => filePath;

/** Display name shown in the UI (stem only, e.g. "analytics"). */
export const getAgentDisplayName = (filePath: string): string =>
  filePath
    .split("/")
    .at(-1)
    ?.replace(/\.agentic\.(yml|yaml)$/i, "") ?? filePath;

const toDisplayProps = (block: AnalyticsDisplayBlock, index: number, runId: string) => {
  const { config, columns, rows } = block;
  const AGENTIC_DATA_KEY = "__agentic_result__";
  const dataKey = `${AGENTIC_DATA_KEY}_${runId}_${index}`;
  const json = JSON.stringify(
    rows.map((row) => Object.fromEntries(columns.map((col, i) => [col, row[i]])))
  );
  const data = { [dataKey]: { file_path: dataKey, json } };

  let display: Parameters<typeof DisplayBlock>[0]["display"];
  const ct = config.chart_type;
  if (ct === "line_chart") {
    display = {
      type: "line_chart",
      x: config.x ?? columns[0] ?? "",
      y: config.y ?? columns[1] ?? "",
      data: dataKey,
      series: config.series,
      title: config.title
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
    display = { type: "table", data: dataKey, title: config.title };
  }

  return { display, data };
};

const AnalyticsDisplayBlockItem = memo(
  ({ block, index, runId }: { block: AnalyticsDisplayBlock; index: number; runId: string }) => {
    const { display, data } = toDisplayProps(block, index, runId);
    return <DisplayBlock display={display} data={data} />;
  }
);

interface AgenticAnalyticsPreviewProps {
  pathb64: string;
}

const AgenticAnalyticsPreview = ({ pathb64 }: AgenticAnalyticsPreviewProps) => {
  const { project, branchName } = useCurrentProjectBranch();
  const threadId = getThreadIdFromPath(project.id, branchName, pathb64);
  const filePath = decodeBase64(pathb64);
  const agentId = getAgentIdFromPath(filePath);
  const displayName = getAgentDisplayName(filePath);

  const { state, start, answer, isStarting, isAnswering } = useAnalyticsRun({
    projectId: project.id
  });

  const [question, setQuestion] = useState("");
  const { formRef, onKeyDown } = useEnterSubmit();

  const isRunning = state.tag === "running" || state.tag === "suspended";
  const hasStarted = state.tag !== "idle";

  const currentEvents: UiBlock[] = "events" in state ? state.events.map(sseEventToUiBlock) : [];

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!question.trim() || isRunning || isStarting) return;
    start(agentId, question, threadId);
    setQuestion("");
  };

  return (
    <div className='relative flex h-full flex-col justify-between overflow-hidden'>
      <div className='customScrollbar scrollbar-gutter-auto flex flex-1 flex-col overflow-auto'>
        <div className='flex flex-col gap-4 p-4'>
          {!hasStarted ? (
            <EmptyState
              className='h-full'
              title='No messages yet'
              description={`Ask the ${displayName} agent a question to get started`}
            />
          ) : (
            <>
              {(currentEvents.length > 0 || isRunning) && (
                <AnalyticsReasoningTrace
                  events={currentEvents}
                  isRunning={isRunning}
                  onSelectArtifact={() => {}}
                />
              )}

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
                <div className='rounded-lg border border-destructive bg-destructive/10 p-4'>
                  <p className='font-medium text-destructive text-sm'>Run failed</p>
                  <Markdown>{state.message}</Markdown>
                </div>
              )}

              {state.tag === "cancelled" && (
                <div className='rounded-lg border border-border bg-muted p-4'>
                  <p className='font-medium text-muted-foreground text-sm'>Operation cancelled</p>
                </div>
              )}
            </>
          )}
        </div>
      </div>

      <div className='p-4'>
        {state.tag === "suspended" ? (
          <SuspensionPrompt
            questions={state.questions}
            onAnswer={answer}
            isAnswering={isAnswering}
          />
        ) : (
          <form
            ref={formRef}
            onSubmit={handleSubmit}
            className='mx-auto flex w-full max-w-[672px] gap-1 rounded-md border p-2'
          >
            <Textarea
              disabled={isRunning || isStarting}
              name='question'
              autoFocus
              onKeyDown={onKeyDown}
              onChange={(e) => setQuestion(e.target.value)}
              value={question}
              className={cx(
                "bg-transparent",
                "border-none shadow-none",
                "hover:border-none focus-visible:border-none focus-visible:shadow-none",
                "focus-visible:ring-0 focus-visible:ring-offset-0",
                "resize-none outline-none",
                "box-border min-h-[32px]"
              )}
              placeholder={`Ask the ${displayName} agent a question`}
            />
            <Button
              className='h-8 w-8'
              disabled={!question || isRunning || isStarting}
              type='submit'
            >
              {isRunning || isStarting ? <Loader2 className='animate-spin' /> : <ArrowUp />}
            </Button>
          </form>
        )}
      </div>
    </div>
  );
};

export default AgenticAnalyticsPreview;
