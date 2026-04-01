import { ChevronDown, ChevronLeft, ChevronRight, Layers, Loader2 } from "lucide-react";
import { useCallback, useState } from "react";
import {
  type AnalyticsStep,
  buildAnalyticsSteps,
  type FanOutGroup,
  type SelectableItem,
  type StepOrGroup
} from "@/hooks/analyticsSteps";
import { cn } from "@/libs/shadcn/utils";
import useAutoCollapse from "@/pages/thread/agentic/BlockMessage/ReasoningTrace/useAutoCollapse";
import type { UiBlock } from "@/services/api/analytics";
import AnalyticsStepRow from "./AnalyticsStepRow";

// ── Fan-out group row ─────────────────────────────────────────────────────────

interface FanOutGroupRowProps {
  group: FanOutGroup;
  onSelectArtifact: (item: SelectableItem) => void;
}

const FanOutGroupRow = ({ group, onSelectArtifact }: FanOutGroupRowProps) => {
  const [activeIndex, setActiveIndex] = useState(0);
  const safeIndex = Math.min(activeIndex, Math.max(0, group.cards.length - 1));
  const activeCard = group.cards[safeIndex];

  return (
    <div className='rounded-md border border-border'>
      {/* Card navigation header */}
      <div className='flex items-center gap-2 border-border border-b px-3 py-1.5'>
        <Layers className='h-3 w-3 shrink-0 text-muted-foreground' />
        <span className='flex-1 text-muted-foreground text-sm'>{group.total} parallel queries</span>
        {group.isStreaming && <Loader2 className='h-3 w-3 animate-spin text-primary' />}

        <div className='flex items-center gap-1'>
          <button
            type='button'
            onClick={() => setActiveIndex((i) => Math.max(0, i - 1))}
            disabled={safeIndex === 0}
            className='rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30'
            aria-label='Previous query'
          >
            <ChevronLeft className='h-3.5 w-3.5' />
          </button>
          <span className='min-w-[3rem] text-center font-mono text-muted-foreground text-xs'>
            {safeIndex + 1} / {group.total}
          </span>
          <button
            type='button'
            onClick={() => setActiveIndex((i) => Math.min(group.cards.length - 1, i + 1))}
            disabled={safeIndex >= group.cards.length - 1}
            className='rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30'
            aria-label='Next query'
          >
            <ChevronRight className='h-3.5 w-3.5' />
          </button>
        </div>
      </div>

      {/* Active card content */}
      <div className='p-2'>
        {activeCard ? (
          activeCard.steps.length > 0 ? (
            <div className='space-y-1'>
              {activeCard.steps.map((step) => (
                <AnalyticsStepRow key={step.id} step={step} onSelectArtifact={onSelectArtifact} />
              ))}
            </div>
          ) : (
            <p className='py-2 text-center text-muted-foreground text-xs'>Running…</p>
          )
        ) : (
          <p className='py-2 text-center text-muted-foreground text-xs'>Waiting for results…</p>
        )}
      </div>

      {/* Dot navigation */}
      {group.total > 1 && (
        <div className='flex justify-center gap-1.5 pb-2'>
          {Array.from({ length: group.total }).map((_, i) => (
            <button
              // biome-ignore lint/suspicious/noArrayIndexKey: index is stable for fixed-count dots
              key={i}
              type='button'
              onClick={() => setActiveIndex(i)}
              aria-label={`Query ${i + 1}`}
              className={cn(
                "h-1.5 rounded-full transition-all",
                i === safeIndex
                  ? "w-4 bg-primary"
                  : "w-1.5 bg-muted-foreground/30 hover:bg-muted-foreground/60"
              )}
            />
          ))}
        </div>
      )}
    </div>
  );
};

// ── Header ────────────────────────────────────────────────────────────────────

function countSteps(items: StepOrGroup[]): { total: number; done: number } {
  let total = 0;
  let done = 0;
  for (const item of items) {
    if (item.kind === "step") {
      total++;
      if (!item.isStreaming) done++;
    } else {
      // fan_out counts as one logical unit
      total++;
      if (!item.isStreaming) done++;
    }
  }
  return { total, done };
}

interface HeaderProps {
  items: StepOrGroup[];
  isStreaming: boolean;
  collapsed: boolean;
  onToggle: () => void;
}

const TraceHeader = ({ items, isStreaming, collapsed, onToggle }: HeaderProps) => {
  const { total, done } = countSteps(items);
  const isComplete = !isStreaming;

  return (
    <button type='button' onClick={onToggle} className='mb-2 flex w-full items-center gap-2'>
      {isComplete ? (
        <ChevronDown
          className={cn(
            "h-3 w-3 shrink-0 text-muted-foreground transition-transform",
            collapsed && "-rotate-90"
          )}
        />
      ) : (
        <Loader2 className='h-3 w-3 shrink-0 animate-spin text-primary' />
      )}
      <span className='font-medium text-muted-foreground text-sm'>Reasoning trace</span>
      <span className='ml-auto font-mono text-muted-foreground text-xs'>
        {isComplete ? `${total} steps` : total > 0 ? `${done}/${total}` : ""}
      </span>
    </button>
  );
};

// ── Root ──────────────────────────────────────────────────────────────────────

interface AnalyticsReasoningTraceProps {
  events: UiBlock[];
  isRunning: boolean;
  onSelectArtifact: (item: SelectableItem) => void;
}

const AnalyticsReasoningTrace = ({
  events,
  isRunning,
  onSelectArtifact
}: AnalyticsReasoningTraceProps) => {
  const items = buildAnalyticsSteps(events);

  const hasContent = items.length > 0;
  const [collapsed, setCollapsed] = useAutoCollapse(isRunning, hasContent);
  const toggleCollapse = useCallback(
    () => !isRunning && setCollapsed((prev) => !prev),
    [isRunning, setCollapsed]
  );

  if (!isRunning && !hasContent) return null;

  return (
    <div className='space-y-1.5 rounded-lg border border-border p-3'>
      <TraceHeader
        items={items}
        isStreaming={isRunning}
        collapsed={collapsed}
        onToggle={toggleCollapse}
      />

      <div
        className={cn(
          "transition-all duration-500",
          collapsed
            ? "max-h-0 overflow-hidden opacity-0"
            : "max-h-[600px] overflow-y-auto opacity-100"
        )}
      >
        <div className='space-y-1.5'>
          {items.map((item) =>
            item.kind === "fan_out" ? (
              <FanOutGroupRow key={item.id} group={item} onSelectArtifact={onSelectArtifact} />
            ) : (
              <AnalyticsStepRow
                key={item.id}
                step={item as AnalyticsStep}
                onSelectArtifact={onSelectArtifact}
              />
            )
          )}
        </div>
      </div>
    </div>
  );
};

export default AnalyticsReasoningTrace;
