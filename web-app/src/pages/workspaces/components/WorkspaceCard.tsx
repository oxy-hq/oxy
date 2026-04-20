import {
  AlertTriangle,
  Bot,
  Check,
  GitCommitHorizontal,
  LayoutDashboard,
  Loader2,
  Pencil,
  Trash2,
  Workflow,
  X
} from "lucide-react";
import type React from "react";
import { useRef, useState } from "react";
import GithubIcon from "@/components/ui/GithubIcon";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger
} from "@/components/ui/shadcn/alert-dialog";
import { Button } from "@/components/ui/shadcn/button";
import { Input } from "@/components/ui/shadcn/input";
import { useRenameWorkspace } from "@/hooks/api/workspaces/useWorkspaces";
import { formatRelativeDate } from "@/libs/utils/date";
import type { WorkspaceSummary } from "@/services/api/workspaces";

interface Props {
  workspace: WorkspaceSummary;
  index: number;
  onSwitch: () => void;
  onDelete: () => void;
  isDeleting: boolean;
}

export function WorkspaceCard({ workspace, index, onSwitch, onDelete, isDeleting }: Props) {
  const createdAt = formatRelativeDate(workspace.created_at);
  const isCloning = workspace.status === "cloning";
  const isErrored = workspace.status === "failed";
  const cloneError = isErrored ? (workspace.error ?? undefined) : undefined;
  const isDisabled = isCloning;

  const [renaming, setRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState("");
  const [renameError, setRenameError] = useState<string | null>(null);
  const renameInputRef = useRef<HTMLInputElement>(null);
  const { mutate: renameWorkspace, isPending: isRenaming } = useRenameWorkspace();

  const startRename = (e: React.MouseEvent) => {
    e.stopPropagation();
    setRenameValue(workspace.name);
    setRenameError(null);
    setRenaming(true);
    setTimeout(() => renameInputRef.current?.select(), 0);
  };

  const commitRename = () => {
    const name = renameValue.trim();
    if (!name || name === workspace.name) {
      setRenaming(false);
      return;
    }
    if (!workspace.org_id) {
      setRenameError("Workspace has no organization");
      return;
    }
    renameWorkspace(
      { orgId: workspace.org_id, id: workspace.id, name },
      {
        onSuccess: () => setRenaming(false),
        onError: (err) => {
          const response = (err as { response?: { data?: unknown; status?: number } })?.response;
          const body = response?.data;
          setRenameError(
            typeof body === "string" && body.length > 0
              ? body
              : response?.status === 409
                ? "A workspace with that name already exists."
                : "Failed to rename workspace"
          );
        }
      }
    );
  };

  return (
    <li
      className='fade-in slide-in-from-bottom-2 h-full animate-in fill-mode-both duration-300'
      style={{ animationDelay: `${index * 60}ms` }}
    >
      <div
        className={`group relative flex h-full flex-col overflow-hidden rounded-xl border transition-all ${
          isCloning
            ? "border-amber-500/25 bg-amber-500/[0.02]"
            : isErrored
              ? "border-destructive/25 bg-destructive/[0.02]"
              : "border-border bg-card hover:border-border/60 hover:shadow-sm"
        }`}
      >
        {/* Cloning shimmer accent line */}
        {isCloning && (
          <div className='absolute top-0 right-0 left-0 h-[2px] overflow-hidden bg-amber-500/20'>
            <div className='h-full w-1/3 animate-[shimmer_1.8s_ease-in-out_infinite] bg-gradient-to-r from-transparent via-amber-400/60 to-transparent' />
          </div>
        )}

        {/* Error accent line */}
        {isErrored && <div className='absolute top-0 right-0 left-0 h-[2px] bg-destructive/40' />}

        {/* Rename overlay — shown instead of the card body button when editing */}
        {renaming && (
          <div className='flex flex-1 flex-col gap-2 p-5'>
            <p className='font-medium text-[13px] text-muted-foreground'>Rename workspace</p>
            <div className='flex items-center gap-1.5'>
              <Input
                ref={renameInputRef}
                value={renameValue}
                onChange={(e) => setRenameValue(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") commitRename();
                  if (e.key === "Escape") setRenaming(false);
                }}
                className='h-7 flex-1 text-sm'
                disabled={isRenaming}
              />
              <Button
                variant='ghost'
                size='icon'
                className='h-6 w-6 shrink-0 text-primary hover:bg-primary/10'
                onClick={commitRename}
                disabled={isRenaming}
              >
                {isRenaming ? (
                  <Loader2 className='h-3 w-3 animate-spin' />
                ) : (
                  <Check className='h-3 w-3' />
                )}
              </Button>
              <Button
                variant='ghost'
                size='icon'
                className='h-6 w-6 shrink-0 text-muted-foreground hover:bg-muted'
                onClick={() => setRenaming(false)}
                disabled={isRenaming}
              >
                <X className='h-3 w-3' />
              </Button>
            </div>
            {renameError && <p className='text-destructive text-xs'>{renameError}</p>}
          </div>
        )}

        {/* Card body */}
        {!renaming && (
          <button
            type='button'
            onClick={isDisabled ? undefined : onSwitch}
            disabled={isDisabled}
            className={`flex flex-1 flex-col gap-2.5 p-5 text-left ${
              isDisabled ? "cursor-not-allowed" : "disabled:opacity-60"
            }`}
          >
            {/* Name + status badge */}
            <div className='flex items-start justify-between gap-3'>
              <span
                className={`font-semibold text-[15px] leading-snug tracking-tight transition-colors ${
                  isCloning || isErrored
                    ? "text-foreground/70"
                    : "text-foreground group-hover:text-foreground"
                }`}
              >
                {workspace.name}
              </span>
              <div className='shrink-0 pt-0.5'>
                {isCloning && (
                  <span className='flex items-center gap-1.5 rounded-full bg-amber-500/10 px-2.5 py-1 font-medium text-[11px] text-amber-600 dark:text-amber-400'>
                    <Loader2 className='h-2.5 w-2.5 animate-spin' />
                    Cloning…
                  </span>
                )}
                {isErrored && (
                  <span className='flex items-center gap-1.5 rounded-full bg-destructive/10 px-2.5 py-1 font-medium text-[11px] text-destructive'>
                    <AlertTriangle className='h-2.5 w-2.5' />
                    Not an Oxy project
                  </span>
                )}
              </div>
            </div>

            {/* Git remote */}
            {workspace.git_remote && !isDisabled && (
              <div className='flex min-w-0 items-center gap-1.5'>
                <GithubIcon className='h-3 w-3 shrink-0 text-muted-foreground/40' />
                <span className='truncate font-mono text-[11px] text-muted-foreground/50'>
                  {workspace.git_remote
                    .replace(/\.git$/, "")
                    .split("/")
                    .slice(-2)
                    .join("/")}
                </span>
              </div>
            )}

            {/* Latest commit */}
            {workspace.git_commit && !isDisabled && (
              <div className='flex min-w-0 items-center gap-1.5'>
                <GitCommitHorizontal className='h-3 w-3 shrink-0 text-muted-foreground/40' />
                <span className='truncate text-[11px] text-muted-foreground/50'>
                  {workspace.git_commit}
                </span>
              </div>
            )}

            {/* Object counts */}
            {!isDisabled &&
              (workspace.agent_count > 0 ||
                workspace.workflow_count > 0 ||
                workspace.app_count > 0) && (
                <div className='flex items-center gap-3 pt-0.5'>
                  {workspace.agent_count > 0 && (
                    <span className='flex items-center gap-1 text-[11px] text-muted-foreground/50'>
                      <Bot className='h-3 w-3' />
                      {workspace.agent_count}
                    </span>
                  )}
                  {workspace.workflow_count > 0 && (
                    <span className='flex items-center gap-1 text-[11px] text-muted-foreground/50'>
                      <Workflow className='h-3 w-3' />
                      {workspace.workflow_count}
                    </span>
                  )}
                  {workspace.app_count > 0 && (
                    <span className='flex items-center gap-1 text-[11px] text-muted-foreground/50'>
                      <LayoutDashboard className='h-3 w-3' />
                      {workspace.app_count}
                    </span>
                  )}
                </div>
              )}

            {/* Cloning notice */}
            {isCloning && (
              <p className='text-[11px] text-amber-600/70 leading-relaxed dark:text-amber-400/60'>
                Repository is being cloned — open once complete.
              </p>
            )}

            {/* Clone error notice */}
            {isErrored && (
              <p className='text-[11px] text-destructive/70 leading-relaxed'>{cloneError}</p>
            )}
          </button>
        )}

        {/* Card footer */}
        <div className='flex items-center justify-between border-border/40 border-t px-5 py-2.5'>
          <div className='flex flex-col gap-0.5'>
            <span className='text-[11px] text-muted-foreground/50'>
              {workspace.git_updated_at
                ? `Updated ${workspace.git_updated_at}`
                : createdAt
                  ? `Created ${createdAt}`
                  : ""}
            </span>
            {workspace.created_by_name && (
              <span className='text-[10px] text-muted-foreground/35'>
                by {workspace.created_by_name}
              </span>
            )}
          </div>

          <div className='flex items-center gap-1'>
            {/* Rename */}
            {!isDisabled && !renaming && (
              <Button
                variant='ghost'
                size='icon'
                onClick={startRename}
                aria-label={`Rename ${workspace.name}`}
                className='h-6 w-6 text-muted-foreground/30 opacity-0 transition-all hover:bg-muted hover:text-foreground group-hover:opacity-100'
              >
                <Pencil className='h-3 w-3' />
              </Button>
            )}

            {/* Delete */}
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button
                  variant='ghost'
                  size='icon'
                  disabled={isDeleting}
                  aria-label={`Delete ${workspace.name}`}
                  className='h-6 w-6 text-muted-foreground/30 opacity-0 transition-all hover:bg-destructive/10 hover:text-destructive group-hover:opacity-100'
                >
                  <Trash2 className='h-3.5 w-3.5' />
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete "{workspace.name}"?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This will permanently delete the workspace and all its files from disk. This
                    action cannot be undone.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>Cancel</AlertDialogCancel>
                  <AlertDialogAction
                    className='bg-destructive text-destructive-foreground hover:bg-destructive/90'
                    onClick={onDelete}
                  >
                    Delete
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </div>
        </div>
      </div>
    </li>
  );
}
