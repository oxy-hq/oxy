import { askPlaceholder } from "@/components/Ask/askPlaceholder";
import ChatPanel from "@/components/Chat/ChatPanel";
import useAskDock from "@/stores/useAskDock";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import { ASK_SUGGESTIONS } from "./constants";

/**
 * The composer view of the dock: suggestion chips (only on a fresh, un-
 * prefilled open) above the ask-locked ChatPanel. Submitting opens the new
 * thread IN the dock (`onThreadCreated`), so the answer streams in place.
 */
export function DockComposer() {
  const prefill = useAskDock((s) => s.prefill);
  const open = useAskDock((s) => s.open);
  const orgName = useCurrentOrg((s) => s.org?.name);
  const wsId = useCurrentWorkspace((s) => s.workspace?.id);

  return (
    <div className='flex flex-col gap-3 p-3'>
      {!prefill?.message && (
        <div className='flex flex-wrap gap-2'>
          {ASK_SUGGESTIONS.map((prompt) => (
            <button
              key={prompt}
              type='button'
              onClick={() => open({ message: prompt })}
              className='rounded-full border bg-background px-3 py-1.5 text-left text-muted-foreground text-xs transition-colors hover:border-primary/40 hover:text-foreground'
            >
              {prompt}
            </button>
          ))}
        </div>
      )}
      <ChatPanel
        // Include the workspace id so switching workspaces remounts the
        // panel — a freely-typed draft (not just a prefill) must not carry
        // over into another workspace's chat (see #2962).
        key={`${wsId}-${prefill?.message ?? "ask"}`}
        lockMode='ask'
        hideAgentPicker
        initialMessage={prefill?.message}
        initialAgentPath={prefill?.agentPath}
        autoSubmit={prefill?.autoSubmit}
        placeholderOverride={askPlaceholder(orgName)}
        onThreadCreated={(id) => useAskDock.getState().openThread(id)}
      />
    </div>
  );
}
