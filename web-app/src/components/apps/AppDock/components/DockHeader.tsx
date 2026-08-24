import { ExternalLink, Maximize2, Minimize2, RotateCw, X } from "lucide-react";
import { AppMark } from "@/components/apps/AppMark";
import { Button } from "@/components/ui/shadcn/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import type { CustomAppSummary } from "@/types/apps";

/**
 * The dock's title bar: identity on the left, the four controls on the right.
 *
 * Split out because `AppDock` was past the ~150-line "review responsibilities"
 * signal in `web-app/CLAUDE.md`, and this is the half with no state of its own —
 * every control is a callback the parent owns, so the seam is where the props
 * already were.
 */
export function DockHeader({
  app,
  focus,
  onReload,
  onToggleFocus,
  onClose
}: {
  app: CustomAppSummary;
  focus: boolean;
  onReload: () => void;
  onToggleFocus: () => void;
  onClose: () => void;
}) {
  return (
    <header className='flex h-10 shrink-0 items-center gap-2 border-b px-3'>
      <AppMark iconUrl={app.icon_url} name={app.name} size='sm' />
      <span className='min-w-0 flex-1 truncate font-medium text-sm'>{app.name}</span>
      <DockAction label='Reload' testId='app-dock-reload' onClick={onReload}>
        <RotateCw className='size-3.5' aria-hidden />
      </DockAction>
      <DockAction
        label={focus ? "Show workspace" : "Focus this app"}
        testId='app-dock-focus'
        onClick={onToggleFocus}
      >
        {focus ? (
          <Minimize2 className='size-3.5' aria-hidden />
        ) : (
          <Maximize2 className='size-3.5' aria-hidden />
        )}
      </DockAction>
      <DockAction label='Open in a new tab' testId='app-dock-popout' href={app.url}>
        <ExternalLink className='size-3.5' aria-hidden />
      </DockAction>
      <DockAction label='Close' testId='app-dock-close' onClick={onClose}>
        <X className='size-3.5' aria-hidden />
      </DockAction>
    </header>
  );
}

/**
 * One header control. A link when it navigates, a button when it acts — the
 * "new tab" affordance has to be a real anchor so middle-click, cmd-click, and
 * "copy link address" all work, and those are exactly how people move an app
 * out of the dock.
 */
function DockAction({
  label,
  testId,
  onClick,
  href,
  children
}: {
  label: string;
  testId: string;
  onClick?: () => void;
  href?: string;
  children: React.ReactNode;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {href ? (
          <a
            href={href}
            target='_blank'
            rel='noreferrer'
            aria-label={label}
            data-testid={testId}
            className='inline-flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-accent hover:text-foreground'
          >
            {children}
          </a>
        ) : (
          <Button
            variant='ghost'
            size='icon'
            aria-label={label}
            data-testid={testId}
            className='size-6 text-muted-foreground hover:text-foreground'
            onClick={onClick}
          >
            {children}
          </Button>
        )}
      </TooltipTrigger>
      <TooltipContent side='bottom'>{label}</TooltipContent>
    </Tooltip>
  );
}
