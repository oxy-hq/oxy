import { AlertTriangle } from "lucide-react";
import { useNavigate } from "react-router-dom";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle
} from "@/components/ui/shadcn/alert-dialog";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import { encodeBase64 } from "@/libs/encoding";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import type { CommitEntry, DirtyEntry, DirtyKind } from "@/services/api";
import useCurrentOrg from "@/stores/useCurrentOrg";
import { useIdeGit } from "../context/IdeGitContext";

// Mirror STATUS_STYLES in ChangesPanel. `staged` collapses to "M".
const KIND_BADGE: Record<DirtyKind, { label: string; className: string }> = {
  modified: { label: "M", className: "text-warning" },
  staged: { label: "M", className: "text-warning" },
  untracked: { label: "A", className: "text-success" },
  deleted: { label: "D", className: "text-destructive" },
  conflicted: { label: "!", className: "text-destructive" }
};

/**
 * State the consequences at the confidence we actually have. "Already on
 * origin" and "never pushed" are very different losses, and "we couldn't
 * check" is a third thing — asserting either of the first two in that case is
 * the overstatement this dialog was rebuilt to avoid.
 */
function describeStakes(total: number, pushed: number, unknown: number): string {
  if (pushed > 0) {
    const [subject, object] = pushed === 1 ? ["is", "it"] : ["are", "them"];
    return `${pushed} of them ${subject} already on origin — dropping ${object} here reverts that work, and pushing afterwards would revert it on the remote too.`;
  }
  if (unknown === total) {
    return "This branch has no upstream, so whether these exist on a remote could not be checked.";
  }
  if (unknown > 0) {
    return `${total - unknown} of them have never been pushed; the remaining ${unknown} could not be checked against a remote.`;
  }
  return "None of them have been pushed, so nothing on the remote changes.";
}

interface Props {
  open: boolean;
  shortHash: string;
  dirty: DirtyEntry[];
  /** Commits the restore would drop. Non-empty for the commit-loss refusal. */
  discardedCommits?: CommitEntry[];
  loading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RestoreConfirmDialog({
  open,
  shortHash,
  dirty,
  discardedCommits = [],
  loading,
  onConfirm,
  onCancel
}: Props) {
  const navigate = useNavigate();
  const { workspaceId } = useIdeGit();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

  // The two guards are mutually exclusive server-side: the commit-loss check
  // runs first and returns before the dirty-tree check.
  const losesCommits = discardedCommits.length > 0;
  // `on_remote` is a tri-state: true (on origin), false (local-only), undefined
  // (no upstream to compare against). Unknown must not be folded in with
  // confirmed-pushed — claiming commits "are already on origin" when we never
  // checked is a softer version of the overstatement this dialog exists to fix.
  const pushedCount = discardedCommits.filter((c) => c.on_remote === true).length;
  const unknownCount = discardedCommits.filter((c) => c.on_remote === undefined).length;

  // Deleted files don't exist on disk; the IDE route would 404.
  const openInIde = (entry: DirtyEntry) => {
    if (entry.kind === "deleted" || !workspaceId) return;
    onCancel();
    navigate(ROUTES.ORG(orgSlug).WORKSPACE(workspaceId).IDE.FILES.FILE(encodeBase64(entry.path)));
  };

  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className='flex items-center gap-2'>
            <AlertTriangle className='h-4 w-4 text-destructive' />
            {losesCommits
              ? `Discard ${discardedCommits.length} commit${discardedCommits.length === 1 ? "" : "s"}?`
              : `Discard ${dirty.length} uncommitted change${dirty.length === 1 ? "" : "s"}?`}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {losesCommits ? (
              <>
                Restoring to <span className='font-mono'>{shortHash}</span> will drop the commits
                below. {describeStakes(discardedCommits.length, pushedCount, unknownCount)} This
                cannot be undone.
              </>
            ) : (
              <>
                Restoring to <span className='font-mono'>{shortHash}</span> will discard the
                following files. Untracked files will be deleted. This cannot be undone.
              </>
            )}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {losesCommits ? (
          <div className='max-h-60 overflow-y-auto rounded border bg-muted/40 py-1'>
            <ul>
              {discardedCommits.map((c) => (
                <li
                  key={c.hash}
                  className='flex items-center gap-2 px-3 py-1 text-left font-mono text-xs'
                >
                  <span className='shrink-0 text-muted-foreground'>{c.short_hash}</span>
                  <span className='truncate text-foreground'>{c.message}</span>
                  {c.on_remote === false && (
                    <span className='ml-auto shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] text-amber-700 dark:text-amber-300'>
                      local only
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ) : (
          <div className='max-h-60 overflow-y-auto rounded border bg-muted/40 py-1'>
            <ul>
              {dirty.map((entry) => {
                const badge = KIND_BADGE[entry.kind] ?? KIND_BADGE.modified;
                const navigable = entry.kind !== "deleted";
                return (
                  <li key={`${entry.kind}:${entry.path}`}>
                    <button
                      type='button'
                      onClick={() => openInIde(entry)}
                      disabled={!navigable}
                      title={navigable ? `Open ${entry.path}` : `${entry.path} (deleted)`}
                      className={cn(
                        "flex w-full items-center gap-2 px-3 py-1 text-left",
                        navigable
                          ? "cursor-pointer hover:bg-accent/40"
                          : "cursor-default opacity-60"
                      )}
                    >
                      <span
                        className={cn(
                          "w-3 shrink-0 text-center font-bold font-mono text-xs uppercase",
                          badge.className
                        )}
                      >
                        {badge.label}
                      </span>
                      <span className='truncate font-mono text-foreground text-xs'>
                        {entry.path}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={loading} onClick={onCancel}>
            Cancel
          </AlertDialogCancel>
          <AlertDialogAction
            disabled={loading}
            onClick={(e) => {
              e.preventDefault();
              onConfirm();
            }}
            className={cn(buttonVariants({ variant: "destructive" }))}
          >
            {loading ? "Restoring…" : "Discard & Restore"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
