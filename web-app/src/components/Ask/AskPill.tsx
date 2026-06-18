import { useLocation } from "react-router-dom";
import { OxyMark } from "@/components/OxyMark";
import useAskPanel from "@/stores/useAskPanel";

/**
 * The platform's always-present entry to the Ask agent. Hovers
 * bottom-center on every Oxy surface; expands into AskPanel on click
 * or Cmd+K. Platform chrome — not a support widget.
 */
export function AskPill() {
  const isOpen = useAskPanel((s) => s.isOpen);
  const open = useAskPanel((s) => s.open);
  const location = useLocation();
  const isThreadDetail = /\/threads\/[^/]+\/?$/.test(location.pathname);
  const isOnboarding = location.pathname.includes("/onboarding");
  if (isOpen || isThreadDetail || isOnboarding) return null;
  return (
    <button
      type='button'
      data-testid='ask-pill'
      onClick={() => open()}
      className='absolute bottom-4 left-1/2 z-40 flex -translate-x-1/2 items-center gap-2 rounded-full border bg-background px-4 py-2 text-muted-foreground text-sm shadow-lg transition-colors hover:border-primary/40 hover:text-foreground'
    >
      <OxyMark className='size-4 text-primary' />
      <span>Ask Oxygen</span>
      <kbd className='rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]'>⌘K</kbd>
    </button>
  );
}
