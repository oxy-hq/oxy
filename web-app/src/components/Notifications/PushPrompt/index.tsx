import { BellRing, X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { cn } from "@/libs/shadcn/utils";
import { HOME_SCREEN_HINT } from "@/libs/webPush";
import { usePushPrompt } from "./usePushPrompt";

interface PushPromptProps {
  /** On a phone, where the row is a full-width tap target. */
  fullScreen?: boolean;
}

/**
 * One row offering push on this device, where people look for notifications:
 * the bell's inbox and the notifications page. Shown only to someone who could
 * turn it on and has not been asked (`pushPromptState`); "×" hides it on this
 * device for 30 days. "Turn on" asks the browser from the click itself.
 */
export function PushPrompt({ fullScreen = false }: PushPromptProps) {
  const prompt = usePushPrompt();
  if (prompt.state === "hidden") return null;

  const homeScreen = prompt.state === "home-screen";
  return (
    <div
      className={cn(
        "flex shrink-0 items-center gap-2 border-b bg-muted/50",
        fullScreen ? "px-3 py-2.5 text-sm" : "px-3 py-2 text-xs"
      )}
      data-testid='push-prompt'
    >
      <BellRing className='size-4 shrink-0 text-muted-foreground' aria-hidden />
      <p
        className='min-w-0 flex-1'
        data-testid={homeScreen ? "push-prompt-home-screen" : undefined}
      >
        {homeScreen ? HOME_SCREEN_HINT : "Get notifications on this device"}
      </p>
      {!homeScreen && (
        <Button
          size='sm'
          className={cn("shrink-0", fullScreen ? "h-8" : "h-7 px-2.5 text-xs")}
          disabled={prompt.pending}
          onClick={prompt.turnOn}
          data-testid='push-prompt-turn-on'
        >
          Turn on
        </Button>
      )}
      <button
        type='button'
        aria-label='Dismiss'
        onClick={prompt.dismiss}
        data-testid='push-prompt-dismiss'
        className='flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground'
      >
        <X className='size-4' />
      </button>
    </div>
  );
}
