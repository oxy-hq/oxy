import { useEffect, useRef } from "react";
import { useLocation } from "react-router-dom";
import ChatPanel from "@/components/Chat/ChatPanel";
import useAskPanel from "@/stores/useAskPanel";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useThreadDrawer from "@/stores/useThreadDrawer";

/** Branded Ask prompt — teaches that Oxygen is the universal interface. */
export const askPlaceholder = (orgName?: string) =>
  `Ask Oxygen anything about ${orgName?.trim() || "your business"}…`;

/** Example questions shown above the composer on a fresh open — kept inside
 *  the panel (not on the home page) so HQ stays calm. One per domain
 *  (sales · labor · QuickBooks), each grounded in the workspace semantic
 *  layer and backed by a passing restaurant_analyst eval so the default
 *  agent answers them reliably. */
const ASK_SUGGESTIONS = [
  "How have net sales trended month over month this year?",
  "How has labor cost trended month over month this year?",
  "What was net operating income in April?"
];

/**
 * Bottom-anchored composer the AskPill expands into. Wraps the existing
 * ChatPanel engine — same thread creation, agent selector, and
 * Ask/Build/Procedure modes. Submitting creates the thread and opens it
 * in the right-side ThreadDrawer — the user never leaves the current
 * page. The close-on-route-change effect remains as a safety net for
 * navigations triggered elsewhere.
 */
export function AskPanel() {
  const { isOpen, prefill, close, open } = useAskPanel();
  const orgName = useCurrentOrg((s) => s.org?.name);
  const location = useLocation();

  const pathRef = useRef(location.pathname);
  useEffect(() => {
    if (location.pathname !== pathRef.current) {
      pathRef.current = location.pathname;
      close();
    }
  }, [location.pathname, close]);

  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented) close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [isOpen, close]);

  if (!isOpen) return null;

  return (
    <>
      <div className='fixed inset-0 z-40 bg-black/20' onClick={close} aria-hidden='true' />
      <div
        data-testid='ask-panel'
        role='dialog'
        aria-label='Ask Oxygen'
        className='absolute bottom-4 left-1/2 z-50 w-[min(672px,calc(100vw-2rem))] -translate-x-1/2'
      >
        {!prefill?.message && (
          <div className='mb-2 flex flex-wrap justify-center gap-2'>
            {ASK_SUGGESTIONS.map((prompt) => (
              <button
                key={prompt}
                type='button'
                onClick={() => open({ message: prompt })}
                className='rounded-full border bg-background px-3 py-1.5 text-muted-foreground text-xs shadow-sm transition-colors hover:border-primary/40 hover:text-foreground'
              >
                {prompt}
              </button>
            ))}
          </div>
        )}
        <div className='rounded-lg shadow-2xl'>
          <ChatPanel
            key={prefill?.message ?? "ask"}
            lockMode='ask'
            hideAgentPicker
            initialMessage={prefill?.message}
            initialAgentPath={prefill?.agentPath}
            autoSubmit={prefill?.autoSubmit}
            placeholderOverride={askPlaceholder(orgName)}
            onThreadCreated={(id) => {
              close();
              useThreadDrawer.getState().open(id);
            }}
          />
        </div>
      </div>
    </>
  );
}
