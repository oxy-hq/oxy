import { Check, ChevronDown, CircleX, Copy } from "lucide-react";
import { useCopyTimeout } from "@/components/automation/output/useCopyTimeout";
import { Button } from "@/components/ui/shadcn/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import { useWorkspaceStatus } from "@/hooks/api/workspaces/useWorkspaceStatus";
import { isIdeRoutingError, isWorkspaceMaterializingError } from "@/libs/utils/ideHealth";

/**
 * Workspace/project configuration status indicator shown in the sidebar and the
 * IDE. When the config fails to load (e.g. a missing `config.yml` after a branch
 * switch), it surfaces the backend error in a click-to-open popover with the
 * full message rendered as selectable, copyable text — not hidden behind a hover
 * tooltip where it cannot be selected or copied.
 */
const WorkspaceConfigStatus = () => {
  const { data, isPending, error } = useWorkspaceStatus();
  const { copied, handleCopy } = useCopyTimeout();

  if (isPending) {
    return null;
  }

  // `/status` is `IdeOnly` — it reads `config.yml` off disk — and this
  // component renders on every NON-IDE page. So on a fleet with no ide wired
  // (`421`) or with one that is down (`502`), every request from here fails,
  // and this banner's words are about the tenant's configuration: it would
  // paint a persistent red "your workspace is broken" for what is a routing
  // fact. Same for the materializing `503`, which is a readiness state.
  //
  // Silence is the honest answer — the global ide banner already reports an
  // outage, in the vocabulary that fits it.
  if (isIdeRoutingError(error) || isWorkspaceMaterializingError(error)) {
    return null;
  }

  if (error) {
    return (
      <div className='flex items-center gap-2 border border-border bg-sidebar p-2'>
        <CircleX className='h-4 w-4 shrink-0 text-destructive' />
        <span className='text-destructive text-sm'>Failed to load workspace status.</span>
      </div>
    );
  }

  if (data?.is_config_valid || !data?.error) {
    return null;
  }

  const errorMessage = data.error;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type='button'
          aria-label='View workspace configuration error'
          className='flex w-full items-center gap-2 border border-border bg-sidebar p-2 text-left transition-colors hover:bg-accent'
        >
          <CircleX className='h-4 w-4 shrink-0 text-destructive' />
          <span className='flex-1 truncate text-destructive text-sm'>
            Unable to load workspace configuration.
          </span>
          <ChevronDown className='h-4 w-4 shrink-0 text-destructive' />
        </button>
      </PopoverTrigger>
      <PopoverContent
        side='bottom'
        align='start'
        className='w-80 max-w-[90vw] space-y-2 bg-card p-3'
      >
        <div className='flex items-center justify-between gap-2'>
          <span className='font-medium text-foreground text-sm'>Configuration error</span>
          <Button
            variant='ghost'
            size='sm'
            className='h-7 gap-1 px-2'
            onClick={() => handleCopy(errorMessage)}
            aria-label='Copy error message'
          >
            {copied ? <Check className='h-3.5 w-3.5' /> : <Copy className='h-3.5 w-3.5' />}
            {copied ? "Copied" : "Copy"}
          </Button>
        </div>
        <pre className='max-h-60 overflow-auto whitespace-pre-wrap break-words rounded-md border border-border bg-muted p-2 text-destructive text-xs'>
          {errorMessage}
        </pre>
      </PopoverContent>
    </Popover>
  );
};

export default WorkspaceConfigStatus;
