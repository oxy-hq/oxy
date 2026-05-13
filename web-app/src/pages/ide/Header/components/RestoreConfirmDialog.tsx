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
import type { DirtyEntry, DirtyKind } from "@/services/api";
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

interface Props {
  open: boolean;
  shortHash: string;
  dirty: DirtyEntry[];
  loading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RestoreConfirmDialog({
  open,
  shortHash,
  dirty,
  loading,
  onConfirm,
  onCancel
}: Props) {
  const navigate = useNavigate();
  const { workspaceId } = useIdeGit();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";

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
            Discard {dirty.length} uncommitted change{dirty.length === 1 ? "" : "s"}?
          </AlertDialogTitle>
          <AlertDialogDescription>
            Restoring to <span className='font-mono'>{shortHash}</span> will discard the following
            files. Untracked files will be deleted. This cannot be undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
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
                      navigable ? "cursor-pointer hover:bg-accent/40" : "cursor-default opacity-60"
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
                    <span className='truncate font-mono text-foreground text-xs'>{entry.path}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
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
