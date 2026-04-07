/**
 * BuilderActivityPanel — opens when the agent proposes a file change.
 *
 * Visualizes the proposed change structurally: for semantic view files it
 * renders dimensions & measures with field-level diffs highlighted.
 * For other files it shows a generic diff summary.
 */
import { Sparkles, X } from "lucide-react";
import { useEffect } from "react";

import type { BuilderActivityItem, BuilderProposedChange } from "@/hooks/useBuilderActivity";

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
  // Show the most-recent pending change; fall back to last change of any status.
  const pendingChange = items
    .filter(
      (i): i is BuilderProposedChange => i.kind === "proposed_change" && i.status === "pending"
    )
    .at(-1);
  const lastChange = items
    .filter((i): i is BuilderProposedChange => i.kind === "proposed_change")
    .at(-1);
  const change = pendingChange ?? lastChange;

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
          <h3 className='font-semibold text-sm'>Proposed Change</h3>
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
