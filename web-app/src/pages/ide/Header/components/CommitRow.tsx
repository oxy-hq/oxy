import { RotateCcw } from "lucide-react";
import { CanWorkspaceAdmin } from "@/components/auth/Can";
import { cn } from "@/libs/shadcn/utils";
import type { CommitEntry } from "@/services/api";

interface Props {
  commit: CommitEntry;
  resettingHash: string | null;
  onReset: (hash: string) => void;
  alwaysShowAction?: boolean;
}

export function CommitRow({ commit, resettingHash, onReset, alwaysShowAction }: Props) {
  return (
    <div className='group flex items-start gap-2 border-b px-3 py-2 last:border-0 hover:bg-accent/40'>
      <div className='min-w-0 flex-1'>
        <p className='truncate text-xs'>{commit.message}</p>
        <p className='flex items-center gap-1 font-mono text-[10px] text-muted-foreground'>
          <span className='truncate'>
            {commit.short_hash} · {commit.author} · {commit.date}
          </span>
          {commit.on_remote === false && (
            <span
              title='This commit exists only in this workspace and has never been pushed. Local-only commits block fast-forward pulls.'
              className='shrink-0 rounded-sm border border-amber-500/40 px-1 text-amber-700 dark:text-amber-300'
            >
              local only
            </span>
          )}
        </p>
      </div>
      <CanWorkspaceAdmin>
        <button
          type='button'
          onClick={() => onReset(commit.hash)}
          disabled={!!resettingHash}
          title={`Restore to ${commit.short_hash}`}
          className={cn(
            "mt-0.5 shrink-0 items-center gap-1 rounded bg-primary px-1.5 py-0.5 text-[10px] text-primary-foreground transition-colors hover:bg-primary/80 disabled:opacity-50",
            alwaysShowAction ? "flex" : "hidden group-hover:flex"
          )}
        >
          <RotateCcw className='h-2.5 w-2.5' />
          {resettingHash === commit.hash ? "…" : "Restore"}
        </button>
      </CanWorkspaceAdmin>
    </div>
  );
}
