import { Hammer, Loader2, Maximize2, Sparkles, X } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { Dialog, DialogContent } from "@/components/ui/shadcn/dialog";
import type { BuilderFileChange } from "@/hooks/useBuilderActivity";
import { useBuilderActivity } from "@/hooks/useBuilderActivity";

import AnalyticsReasoningTrace from "../AnalyticsReasoningTrace";
import ChangeVisualization from "../BuilderActivityPanel/ChangeVisualization";
import { useBuilderDelegationEvents } from "./useBuilderDelegationEvents";

interface BuilderDelegationPanelProps {
  childRunId: string;
  projectId: string;
  onClose: () => void;
}

export default function BuilderDelegationPanel({
  childRunId,
  projectId,
  onClose
}: BuilderDelegationPanelProps) {
  const { events, isStreaming } = useBuilderDelegationEvents(projectId, childRunId, true);
  const [fullscreen, setFullscreen] = useState(false);

  // Empty — auto-accept means no manual accept/reject decisions in this panel.
  const changeDecisions = useMemo(() => new Map<number, "accepted" | "rejected">(), []);
  const activityItems = useBuilderActivity(events, changeDecisions);

  // Builder child runs don't nest further delegations — no-op is sufficient.
  const onSelectArtifact = useCallback(() => {}, []);

  // Show the most-recent proposed change (pending first, then last of any status).
  const pendingChange = activityItems
    .filter((i): i is BuilderFileChange => i.kind === "file_changed" && i.status === "pending")
    .at(-1);
  const lastChange = activityItems
    .filter((i): i is BuilderFileChange => i.kind === "file_changed")
    .at(-1);
  const change = pendingChange ?? lastChange;

  const subtitle = isStreaming ? "Working on semantic layer changes…" : "Completed";

  return (
    <div className='flex h-full flex-col border-l bg-background'>
      {/* Header */}
      <div className='flex shrink-0 items-center justify-between border-b px-4 py-3'>
        <div className='flex min-w-0 items-center gap-2'>
          {isStreaming ? (
            <Loader2 className='h-4 w-4 shrink-0 animate-spin text-primary' />
          ) : (
            <Hammer className='h-4 w-4 shrink-0 text-primary' />
          )}
          <div className='min-w-0'>
            <h3 className='font-semibold text-sm'>Builder Agent</h3>
            <p className='mt-0.5 text-[11px] text-muted-foreground'>{subtitle}</p>
          </div>
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

      {/* Reasoning trace + change visualization */}
      <div className='flex min-h-0 flex-1 flex-col overflow-y-auto p-4'>
        {(events.length > 0 || isStreaming) && (
          <div className='mb-4'>
            <AnalyticsReasoningTrace
              events={events}
              isRunning={isStreaming}
              onSelectArtifact={onSelectArtifact}
            />
          </div>
        )}

        {change ? (
          <>
            {/* Change section header with fullscreen button */}
            <div className='mb-2 flex items-center justify-between'>
              <span className='font-medium text-muted-foreground text-xs'>Proposed change</span>
              <button
                type='button'
                onClick={() => setFullscreen(true)}
                className='rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
                aria-label='Fullscreen'
              >
                <Maximize2 className='h-3.5 w-3.5' />
              </button>
            </div>
            <div className='min-h-0 flex-1'>
              <ChangeVisualization change={change} />
            </div>

            {/* Fullscreen dialog — graph only, no header text */}
            <Dialog open={fullscreen} onOpenChange={setFullscreen}>
              <DialogContent
                className='flex h-[80vh] w-[80vw] max-w-[80vw] flex-col gap-0 p-4'
                showCloseButton={false}
              >
                <div className='relative min-h-0 flex-1 overflow-hidden'>
                  <ChangeVisualization change={change} />
                  <button
                    type='button'
                    onClick={() => setFullscreen(false)}
                    className='absolute top-2 right-2 rounded p-1 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground'
                    aria-label='Close fullscreen'
                  >
                    <X className='h-4 w-4' />
                  </button>
                </div>
              </DialogContent>
            </Dialog>
          </>
        ) : (
          !isStreaming && (
            <div className='flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center'>
              <div className='rounded-full bg-muted p-3'>
                <Sparkles className='h-5 w-5 text-muted-foreground' />
              </div>
              <p className='text-muted-foreground text-sm'>No changes proposed.</p>
            </div>
          )
        )}
      </div>
    </div>
  );
}
