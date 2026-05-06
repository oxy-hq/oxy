/**
 * BuilderActivityPanel — opens when the agent proposes a file change.
 *
 * When multiple files are changed in one LLM turn, shows tabs so the user
 * can review each diff before accepting or rejecting all of them.
 */
import { Sparkles, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { BuilderActivityItem, BuilderFileChange } from "@/hooks/useBuilderActivity";
import { cn } from "@/libs/shadcn/utils";

import ChangeVisualization from "./ChangeVisualization";

// ── Panel ─────────────────────────────────────────────────────────────────────

export interface BuilderActivityPanelProps {
  items: BuilderActivityItem[];
  isRunning: boolean;
  isSuspended: boolean;
  onAnswer: (text: string) => void;
  isAnswering: boolean;
  onClose: () => void;
}

const BuilderActivityPanel = ({
  items,
  isRunning,
  isSuspended,
  onClose
}: BuilderActivityPanelProps) => {
  const pendingChanges = items.filter(
    (i): i is BuilderFileChange => i.kind === "file_changed" && i.status === "pending"
  );
  // Fall back to last change of any status when nothing is pending.
  const displayChanges =
    pendingChanges.length > 0
      ? pendingChanges
      : items.filter((i): i is BuilderFileChange => i.kind === "file_changed").slice(-1);

  const [activeIndex, setActiveIndex] = useState(0);
  const safeIndex = Math.min(activeIndex, Math.max(0, displayChanges.length - 1));
  const change = displayChanges[safeIndex] ?? null;

  // Reset tab to the last item when new pending changes arrive.
  useEffect(() => {
    if (pendingChanges.length > 0) {
      setActiveIndex(pendingChanges.length - 1);
    }
  }, [pendingChanges.length]);

  useEffect(() => {
    if (change?.status === "accepted") onClose();
  }, [change?.status, onClose]);

  const subtitle = isSuspended
    ? "Review the proposed change"
    : isRunning
      ? "Agent is working…"
      : "Done";

  return (
    <div className='flex h-full flex-col border-l bg-background'>
      {/* Header */}
      <div className='flex shrink-0 items-center justify-between border-b px-4 py-3'>
        <div className='min-w-0 flex-1'>
          <h3 className='font-semibold text-sm'>File Change</h3>
          <p className='mt-0.5 text-[11px] text-muted-foreground'>{subtitle}</p>
        </div>
        <button
          type='button'
          onClick={onClose}
          className='ml-2 rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
          aria-label='Close panel'
        >
          <X className='h-4 w-4' />
        </button>
      </div>

      {/* Tab strip — only shown when there are 2+ display changes */}
      {displayChanges.length > 1 && (
        <div className='flex shrink-0 items-center gap-1 overflow-x-auto border-border border-b px-2 py-1.5'>
          {displayChanges.map((c, i) => (
            <button
              key={c.id}
              type='button'
              onClick={() => setActiveIndex(i)}
              className={cn(
                "shrink-0 rounded px-2.5 py-1 font-mono text-xs transition-colors",
                i === safeIndex
                  ? "bg-accent text-foreground"
                  : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
              )}
            >
              {c.filePath.split("/").pop() ?? c.filePath}
            </button>
          ))}
        </div>
      )}

      {/* Change visualization */}
      {change ? (
        <div className='flex min-h-0 flex-1 flex-col overflow-hidden p-4'>
          <div className='h-full'>
            <ChangeVisualization change={change} />
          </div>
        </div>
      ) : (
        <div className='flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center'>
          <div className='rounded-full bg-muted p-3'>
            <Sparkles className='h-5 w-5 text-muted-foreground' />
          </div>
          <p className='text-muted-foreground text-sm'>No change proposed yet.</p>
        </div>
      )}
    </div>
  );
};

export default BuilderActivityPanel;
