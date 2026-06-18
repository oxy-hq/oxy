import { AlertCircle, ArrowRight, X } from "lucide-react";
import { useState } from "react";
import type { SetupGap } from "../useWorkspaceReadiness";

export const ProjectSetupToast = ({ gaps }: { gaps: SetupGap[] }) => {
  const [dismissed, setDismissed] = useState(false);
  if (dismissed || gaps.length === 0) return null;

  return (
    <div className='fade-in slide-in-from-top-2 fixed top-16 right-4 z-50 w-96 animate-in duration-300'>
      <div className='rounded-lg border border-amber-500/30 bg-background shadow-black/10 shadow-lg'>
        <div className='flex items-center justify-between border-amber-500/20 border-b px-4 py-3'>
          <div className='flex items-center gap-2 text-amber-600 dark:text-amber-400'>
            <AlertCircle className='h-3.5 w-3.5 shrink-0' />
            <span className='font-medium text-xs'>Project setup incomplete</span>
          </div>
          <button
            type='button'
            onClick={() => setDismissed(true)}
            className='rounded p-0.5 text-muted-foreground/40 transition-colors hover:bg-muted hover:text-muted-foreground'
            aria-label='Dismiss'
          >
            <X className='h-3.5 w-3.5' />
          </button>
        </div>
        <div className='flex flex-col gap-1 p-2'>
          {gaps.map((gap) => (
            <div
              key={gap.label}
              className='flex items-center justify-between gap-3 rounded-md px-2 py-2'
            >
              <div className='flex min-w-0 items-center gap-2 text-muted-foreground text-xs'>
                <gap.icon className='h-3 w-3 shrink-0' />
                <span className='truncate'>{gap.label}</span>
              </div>
              <button
                type='button'
                onClick={gap.action}
                className='flex shrink-0 items-center gap-1 whitespace-nowrap font-medium text-primary text-xs hover:underline'
              >
                {gap.cta}
                <ArrowRight className='h-3 w-3' />
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
