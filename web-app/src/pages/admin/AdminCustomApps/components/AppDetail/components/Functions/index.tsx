import { ChevronDown, ChevronRight, Clock, Code2, KeyRound } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import { useAppFunctions } from "@/hooks/api/customApps/useAppFunctions";
import type { AppFunctionSummary } from "@/types/apps";
import { FunctionDetail } from "./FunctionDetail";

/**
 * The AppDetail "Functions" section: the app's Oxy Functions in its active
 * build, each expandable to its manifest config, recent invocation history,
 * and a "Run now" trigger that surfaces the resulting job run's logs. Manage +
 * debug the code-first functions shipped via `oxy publish`.
 */
export const Functions = ({ appId }: { appId: string }) => {
  const { data: functions, isLoading, error } = useAppFunctions(appId);

  if (isLoading) {
    return <p className='text-muted-foreground text-sm'>Loading functions…</p>;
  }
  if (error) {
    return <p className='text-destructive text-sm'>Couldn't load functions.</p>;
  }
  if (!functions || functions.length === 0) {
    return (
      <p className='text-muted-foreground text-sm'>
        This app ships no Oxy Functions. Add a <code>functions/&lt;name&gt;.ts</code> to the bundle
        and <code>oxy publish</code>.
      </p>
    );
  }

  return (
    <ul className='flex flex-col gap-1.5'>
      {functions.map((fn) => (
        <FunctionRow key={fn.name} appId={appId} fn={fn} />
      ))}
    </ul>
  );
};

const FunctionRow = ({ appId, fn }: { appId: string; fn: AppFunctionSummary }) => {
  const [open, setOpen] = useState(false);
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <li className='rounded-md border border-border bg-card'>
      <button
        type='button'
        onClick={() => setOpen((o) => !o)}
        className='flex w-full items-center gap-2 px-3 py-2 text-left'
        aria-expanded={open}
      >
        <Chevron className='size-3.5 shrink-0 text-muted-foreground' />
        <Code2 className='size-3.5 shrink-0 text-vis-violet' />
        <span className='truncate font-mono text-sm'>{fn.name}</span>
        <div className='ml-auto flex shrink-0 items-center gap-1'>
          <SurfaceBadges fn={fn} />
        </div>
      </button>
      {open && <FunctionDetail appId={appId} fn={fn} />}
    </li>
  );
};

/** Which invocation surfaces the function declares — the at-a-glance identity. */
const SurfaceBadges = ({ fn }: { fn: AppFunctionSummary }) => (
  <>
    {fn.route && (
      <Badge variant='secondary' className='h-5'>
        Route
      </Badge>
    )}
    {fn.schedule && (
      <Badge variant='secondary' className='h-5 gap-1'>
        <Clock className='size-3' />
        Scheduled
      </Badge>
    )}
    {fn.airway && (
      <Badge variant='secondary' className='h-5'>
        Airway
      </Badge>
    )}
    {fn.secrets_write && (
      <Badge variant='secondary' className='h-5 gap-1' title='Writes app-scoped secrets'>
        <KeyRound className='size-3' />
        Secrets
      </Badge>
    )}
  </>
);
