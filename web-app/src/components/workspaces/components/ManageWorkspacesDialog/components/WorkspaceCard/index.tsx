import { GitCommitHorizontal } from "lucide-react";
import GithubIcon from "@/components/ui/GithubIcon";
import { cn } from "@/libs/shadcn/utils";
import { formatRelativeDate } from "@/libs/utils/date";
import type { WorkspaceSummary } from "@/services/api/workspaces";
import { CardFooter } from "./components/CardFooter";
import { RenameForm } from "./components/RenameForm";
import { StatusBadge } from "./components/StatusBadge";
import { WorkspaceStats } from "./components/WorkspaceStats";
import { useRenameForm } from "./useRenameForm";

interface Props {
  workspace: WorkspaceSummary;
  index: number;
  isActive: boolean;
  onSwitch: () => void;
  onDelete: () => void;
  isDeleting: boolean;
}

export function WorkspaceCard({
  workspace,
  index,
  isActive,
  onSwitch,
  onDelete,
  isDeleting
}: Props) {
  const isCloning = workspace.status === "cloning";
  const isErrored = workspace.status === "failed";
  const isDisabled = isCloning;
  const createdAt = formatRelativeDate(workspace.created_at);
  const rename = useRenameForm(workspace);

  return (
    <div
      className='fade-in slide-in-from-bottom-2 h-full animate-in fill-mode-both duration-300'
      style={{ animationDelay: `${index * 60}ms` }}
    >
      <div className={getCardClassName({ isCloning, isErrored, isActive })}>
        {isCloning && <CloningStripe />}
        {isErrored && <div className='absolute top-0 right-0 left-0 h-0.5 bg-destructive/40' />}

        {rename.isOpen ? (
          <RenameForm
            value={rename.value}
            onChange={rename.setValue}
            onSubmit={rename.submit}
            onCancel={rename.close}
            isPending={rename.isPending}
            error={rename.error}
            inputRef={rename.inputRef}
          />
        ) : (
          <button
            type='button'
            onClick={isDisabled ? undefined : onSwitch}
            disabled={isDisabled}
            className={cn(
              "flex flex-1 flex-col gap-2.5 p-5 text-left",
              isDisabled ? "cursor-not-allowed" : "disabled:opacity-60"
            )}
          >
            <div className='flex items-start justify-between gap-3'>
              <span
                className={cn(
                  "font-semibold text-base leading-snug tracking-tight transition-colors",
                  isCloning || isErrored ? "text-foreground/70" : "text-foreground"
                )}
              >
                {workspace.name}
              </span>
              <div className='flex shrink-0 items-center gap-1.5 pt-0.5'>
                <StatusBadge isActive={isActive} isCloning={isCloning} isErrored={isErrored} />
              </div>
            </div>

            {!isDisabled && workspace.git_remote && (
              <div className='flex min-w-0 items-center gap-1.5'>
                <GithubIcon className='size-3 shrink-0 text-muted-foreground/40' />
                <span className='truncate font-mono text-muted-foreground/50 text-xs'>
                  {formatGitRemote(workspace.git_remote)}
                </span>
              </div>
            )}

            {!isDisabled && workspace.git_commit && (
              <div className='flex min-w-0 items-center gap-1.5'>
                <GitCommitHorizontal className='size-3 shrink-0 text-muted-foreground/40' />
                <span className='truncate text-muted-foreground/50 text-xs'>
                  {workspace.git_commit}
                </span>
              </div>
            )}

            {!isDisabled && <WorkspaceStats workspace={workspace} />}

            {isCloning && (
              <p className='text-warning/70 text-xs leading-relaxed'>
                Repository is being cloned — open once complete.
              </p>
            )}

            {!isErrored && workspace.error && (
              <p className='text-destructive/70 text-xs leading-relaxed'>{workspace.error}</p>
            )}
          </button>
        )}

        <CardFooter
          workspace={workspace}
          createdAt={createdAt}
          showRename={!isDisabled && !rename.isOpen}
          isDeleting={isDeleting}
          onRename={rename.open}
          onDelete={onDelete}
        />
      </div>
    </div>
  );
}

function getCardClassName({
  isCloning,
  isErrored,
  isActive
}: {
  isCloning: boolean;
  isErrored: boolean;
  isActive: boolean;
}) {
  return cn(
    "group relative flex h-full flex-col overflow-hidden rounded-xl border transition-all",
    isCloning && "border-warning/25 bg-warning/5",
    isErrored && "border-destructive/25 bg-destructive/5",
    !isCloning && !isErrored && isActive && "border-primary/40 bg-primary/5",
    !isCloning &&
      !isErrored &&
      !isActive &&
      "border-border bg-card hover:border-border/60 hover:shadow-sm"
  );
}

function CloningStripe() {
  return (
    <div className='absolute top-0 right-0 left-0 h-0.5 overflow-hidden bg-warning/20'>
      <div className='h-full w-1/3 animate-[shimmer_1.8s_ease-in-out_infinite] bg-gradient-to-r from-transparent via-warning/60 to-transparent' />
    </div>
  );
}

function formatGitRemote(remote: string) {
  return remote
    .replace(/\.git$/, "")
    .split("/")
    .slice(-2)
    .join("/");
}
