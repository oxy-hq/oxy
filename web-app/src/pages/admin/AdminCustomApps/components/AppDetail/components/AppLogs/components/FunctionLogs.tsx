import { Skeleton } from "@/components/ui/shadcn/skeleton";
import { useAppLogs } from "@/hooks/api/customApps/useCustomApps";
import type { FunctionLogLine } from "@/types/apps";

/** Level → the muted/foreground/destructive ladder. No raw colors. */
const levelClass = (level: string) => {
  if (level === "error") return "text-destructive";
  if (level === "warn") return "text-foreground";
  return "text-muted-foreground";
};

const shortTime = (iso: string) => iso.slice(11, 19) || iso;

export const FunctionLogs = ({ orgSlug, appSlug }: { orgSlug: string; appSlug: string }) => {
  const { data, isLoading, error } = useAppLogs(orgSlug, appSlug);

  if (isLoading) return <Skeleton className='h-24 w-full' />;
  if (error) {
    return (
      <p className='text-muted-foreground text-xs' data-testid='admin-app-logs-error'>
        Could not read function logs.
      </p>
    );
  }
  const logs = data ?? [];
  if (logs.length === 0) {
    return (
      <p className='text-muted-foreground text-xs' data-testid='admin-app-logs-empty'>
        No function output in the last 24 hours. An app with no Oxy Functions, and one whose
        functions printed nothing, both look like this.
      </p>
    );
  }

  return (
    <div
      className='max-h-80 overflow-auto rounded-md border bg-muted/20'
      data-testid='admin-app-logs-list'
    >
      {logs.map((line: FunctionLogLine) => (
        <div
          key={`${line.invocation_id}-${line.seq}`}
          className='flex gap-2 border-b px-3 py-1 font-mono text-xs last:border-b-0'
        >
          <span className='shrink-0 text-muted-foreground tabular-nums'>
            {shortTime(line.timestamp)}
          </span>
          <span className='shrink-0 text-muted-foreground'>{line.function_name}</span>
          <span className={`min-w-0 whitespace-pre-wrap break-words ${levelClass(line.level)}`}>
            {line.message}
          </span>
        </div>
      ))}
    </div>
  );
};
