import { askPlaceholder } from "@/components/Ask/AskPanel";
import ChatPanel from "@/components/Chat/ChatPanel";
import { cn } from "@/libs/shadcn/utils";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useThreadDrawer from "@/stores/useThreadDrawer";

const getGreeting = () => {
  const hour = new Date().getHours();
  if (hour < 12) return "Good Morning";
  if (hour < 18) return "Good Afternoon";
  return "Good Evening";
};

/**
 * Empty-state home for workspaces with zero published custom apps
 * (every `oxy init` user): the classic greeting + composer, so the
 * launcher never renders a dead grid.
 */
export function AskForwardFallback({ shouldDisableChat }: { shouldDisableChat: boolean }) {
  const orgName = useCurrentOrg((s) => s.org?.name);
  return (
    <div className='flex flex-1 flex-col items-center justify-center gap-6 px-4 sm:gap-10'>
      <p className='text-balance text-center text-xl sm:text-3xl'>
        {getGreeting()}! How can I assist you?
      </p>
      <div className='flex w-full max-w-4xl flex-col items-center gap-3'>
        {shouldDisableChat && (
          <p className='text-center text-muted-foreground/50 text-xs'>
            Complete the setup steps above to start chatting.
          </p>
        )}
        <div
          className={cn(
            "w-full",
            shouldDisableChat && "pointer-events-none select-none opacity-40"
          )}
        >
          <ChatPanel
            lockMode='ask'
            hideAgentPicker
            placeholderOverride={askPlaceholder(orgName)}
            onThreadCreated={(id) => useThreadDrawer.getState().open(id)}
          />
        </div>
      </div>
    </div>
  );
}
