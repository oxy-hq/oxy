import { useMemo } from "react";
import { askPlaceholder } from "@/components/Ask/askPlaceholder";
import ChatPanel from "@/components/Chat/ChatPanel";
import { useCustomApps } from "@/hooks/api/customApps/useCustomApps";
import useAskDock from "@/stores/useAskDock";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

/** Chips shown above the composer. Three fits one row at the dock's width. */
const MAX_SUGGESTIONS = 3;

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

  // Suggestions come from THIS workspace's own apps (`ask.suggestedQuestions`
  // in each app manifest), never from a constant.
  //
  // They used to be a hardcoded array written against Poke House's
  // restaurant_analyst evals, which meant every other tenant was offered a
  // different customer's business questions — "How has labor cost trended
  // month over month?" in front of a first-aid distributor. That is a tenant
  // leak wearing the costume of a placeholder, and it blocks inviting a
  // customer into their own workspace.
  //
  // There is deliberately NO fallback. A generic default is exactly what
  // produced the bug: whatever gets written there is some tenant's domain, and
  // it renders for all the others. A workspace that has declared no questions
  // shows no chips, which is honest and empty rather than confident and wrong.
  // Each chip carries the agent of the app that declared it. `ask.agent` and
  // `ask.suggestedQuestions` are authored together in one manifest block, and
  // `OxyShell` reads them together for the in-app dock — splitting them here
  // would run a question against whichever agent the workspace defaults to.
  // "Why does Pleasanton rank #1?" means nothing to an agent that never saw
  // that app's semantic scope, and a confident chip returning a confused answer
  // is a softer version of the failure this component exists to stop.
  //
  // De-duplicated on the TRIMMED question, which is also the key: the same
  // question from two apps with different incidental whitespace is one chip,
  // not two that both render and eat two of the three slots.
  const { data: apps = [], isPending } = useCustomApps(wsId ?? "");
  const suggestions = useMemo(() => {
    const seen = new Set<string>();
    const picked: { question: string; agentPath?: string }[] = [];
    for (const app of apps) {
      for (const raw of app.suggested_questions ?? []) {
        const question = raw.trim();
        if (!question || seen.has(question)) continue;
        seen.add(question);
        picked.push({ question, agentPath: app.default_agent });
        if (picked.length >= MAX_SUGGESTIONS) return picked;
      }
    }
    return picked;
  }, [apps]);

  return (
    <div className='flex flex-col gap-3 p-3'>
      {/* Held back until the fetch settles rather than rendering an empty row
          that fills in and shoves the composer down. The key is usually warm
          from the launcher, but a deep link straight to a workspace is not. */}
      {!prefill?.message && !isPending && suggestions.length > 0 && (
        <div className='flex flex-wrap gap-2' data-testid='ask-suggestions'>
          {suggestions.map(({ question, agentPath }) => (
            <button
              key={question}
              type='button'
              onClick={() => open({ message: question, agentPath })}
              className='rounded-full border bg-background px-3 py-1.5 text-left text-muted-foreground text-xs transition-colors hover:border-primary/40 hover:text-foreground'
            >
              {question}
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
