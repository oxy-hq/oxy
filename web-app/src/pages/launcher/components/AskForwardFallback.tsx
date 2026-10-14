import { askPlaceholder } from "@/components/Ask/askPlaceholder";
import ChatPanel from "@/components/Chat/ChatPanel";
import { cn } from "@/libs/shadcn/utils";
import { getGreeting } from "@/libs/utils/greeting";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";

/**
 * Empty-state home for an org with zero published custom apps (every
 * `oxy init` user): the classic greeting + composer, so the launcher never
 * renders a dead grid. Submitting navigates to the full thread page (the Ask
 * dock is the surface for in-context asks).
 *
 * `secondary` is the same composer under the apps of the org's other
 * workspaces: a short line instead of the greeting, not centred in the page.
 */
export function AskForwardFallback({
  shouldDisableChat,
  hasSetupSteps,
  secondary = false
}: {
  shouldDisableChat: boolean;
  /** Whether the setup toast lists steps this viewer can take. */
  hasSetupSteps: boolean;
  secondary?: boolean;
}) {
  const orgName = useCurrentOrg((s) => s.org?.name);
  // The workspace switcher navigates to this workspace's ROOT (the
  // launcher), which is the SAME route/element across a switch — so this
  // component instance is reused rather than remounted. Key ChatPanel by
  // workspace id so a typed draft can't survive the switch (see #2962).
  const wsId = useCurrentWorkspace((s) => s.workspace?.id);
  return (
    <div
      className={cn(
        "flex flex-col items-center px-4",
        secondary ? "gap-3 pb-8" : "flex-1 justify-center gap-6 sm:gap-10"
      )}
      data-testid={secondary ? "launcher-ask-secondary" : "launcher-ask-fallback"}
    >
      <p
        className={cn(
          "text-balance text-center",
          secondary ? "text-muted-foreground text-sm" : "text-xl sm:text-3xl"
        )}
      >
        {secondary ? "Or ask a question" : `${getGreeting()}! How can I assist you?`}
      </p>
      <div
        className={cn(
          "flex w-full flex-col items-center gap-3",
          secondary ? "max-w-3xl" : "max-w-4xl"
        )}
      >
        {shouldDisableChat && (
          <p
            className='text-center text-muted-foreground/50 text-xs'
            data-testid='launcher-chat-locked-reason'
          >
            {hasSetupSteps
              ? "Complete the setup steps above to start chatting."
              : "Chat isn't set up for this workspace yet."}
          </p>
        )}
        <div
          className={cn(
            "w-full",
            shouldDisableChat && "pointer-events-none select-none opacity-40"
          )}
        >
          <ChatPanel
            key={wsId}
            lockMode='ask'
            hideAgentPicker
            placeholderOverride={askPlaceholder(orgName)}
          />
        </div>
      </div>
    </div>
  );
}
