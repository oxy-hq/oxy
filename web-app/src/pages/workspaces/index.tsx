import {
  AlertTriangle,
  BookOpen,
  Bot,
  Check,
  Database,
  GitCommitHorizontal,
  GitMerge,
  KeyRound,
  LayoutDashboard,
  Loader2,
  Pencil,
  Plus,
  Trash2,
  Workflow,
  X
} from "lucide-react";
import type React from "react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
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
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/shadcn/dialog";
import { Input } from "@/components/ui/shadcn/input";
import { Label } from "@/components/ui/shadcn/label";
import {
  useActivateWorkspace,
  useAllWorkspaces,
  useDeleteWorkspace,
  useRenameWorkspace
} from "@/hooks/api/workspaces/useWorkspaces";
import ROUTES from "@/libs/utils/routes";
import { GitHubOnboardingStep } from "@/pages/onboarding/GitHubOnboardingStep";
import { OnboardingService } from "@/services/api/onboarding";
import type { WorkspaceSummary } from "@/services/api/workspaces";

// ─── Post-creation reminder dialog ────────────────────────────────────────────

type WorkspaceCreationType = "demo" | "new" | "github";

function WorkspaceReminderDialog({
  open,
  workspaceId,
  workspaceType,
  onClose
}: {
  open: boolean;
  workspaceId: string | null;
  workspaceType: WorkspaceCreationType | null;
  onClose: () => void;
}) {
  const routes = workspaceId ? ROUTES.WORKSPACE(workspaceId) : null;
  const isDemo = workspaceType === "demo";
  const isGithub = workspaceType === "github";

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <div className='mb-1 flex h-9 w-9 items-center justify-center rounded-lg border border-primary/20 bg-primary/5'>
            <GitMerge className='h-4 w-4 text-primary' />
          </div>
          <DialogTitle className='font-semibold text-base'>
            {isDemo
              ? "Demo workspace ready"
              : isGithub
                ? "Repository imported"
                : "Workspace created"}
          </DialogTitle>
        </DialogHeader>

        <p className='text-muted-foreground text-sm leading-relaxed'>
          {isDemo
            ? "Your demo workspace is set up with sample data. Here's what's included and what you can configure:"
            : isGithub
              ? "The repository is being cloned in the background. While you wait, set up a few things:"
              : "Your workspace is ready. Set up a few things to get the most out of Oxy:"}
        </p>

        <div className='flex flex-col gap-2'>
          <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
            <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
              <KeyRound className='h-3 w-3 text-muted-foreground' />
            </div>
            <div className='min-w-0 flex-1'>
              <p className='font-medium text-[13px] text-foreground'>LLM API key</p>
              <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                Add your LLM provider key in Settings → Secrets.
              </p>
            </div>
            {routes && (
              <a
                href={routes.IDE.SETTINGS.SECRETS}
                target='_blank'
                rel='noopener noreferrer'
                className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
              >
                Set up ↗
              </a>
            )}
          </div>

          <div className='flex items-start gap-3 rounded-lg border border-border bg-muted/30 px-3.5 py-3'>
            <div className='mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-md border border-border bg-background'>
              <Database className='h-3 w-3 text-muted-foreground' />
            </div>
            <div className='min-w-0 flex-1'>
              {isDemo ? (
                <>
                  <p className='font-medium text-[13px] text-foreground'>Sample data included</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    Pre-loaded DuckDB databases with retail and sales data. No setup required.
                  </p>
                </>
              ) : (
                <>
                  <p className='font-medium text-[13px] text-foreground'>Database connection</p>
                  <p className='mt-0.5 text-muted-foreground text-xs leading-relaxed'>
                    Add a database so agents can run SQL queries.
                  </p>
                </>
              )}
            </div>
            {!isDemo && routes && (
              <a
                href={routes.IDE.SETTINGS.DATABASES}
                target='_blank'
                rel='noopener noreferrer'
                className='mt-0.5 shrink-0 text-primary text-xs hover:underline'
              >
                Set up ↗
              </a>
            )}
          </div>
        </div>

        {isGithub && (
          <p className='text-muted-foreground/60 text-xs'>
            You can activate the workspace once cloning completes. The card will update
            automatically.
          </p>
        )}

        <div className='flex justify-end gap-2 pt-1'>
          <Button onClick={onClose} size='sm' className='h-8 px-4 text-xs'>
            Got it
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// Extract a human-readable message from an API error, preferring the response body.
// For 409 Conflict, returns a specific duplicate-name message if the body isn't readable.
// Returns null (not the generic Axios "Request failed…" message) when nothing is available.
function extractErrorMessage(err: unknown): string | null {
  if (!err || typeof err !== "object") return null;
  const response = (err as { response?: { data?: unknown; status?: number } })?.response;
  const body = response?.data;
  if (typeof body === "string" && body.length > 0) return body;
  if (response?.status === 409) {
    return "A workspace with that name already exists. Please choose a different name.";
  }
  return null;
}

// ─── Create workspace dialog ────────────────────────────────────────────────────

type CreateStep = "pick" | "new" | "github" | "loading";

function CreateWorkspaceDialog({
  open,
  onClose,
  onCreated
}: {
  open: boolean;
  onClose: () => void;
  onCreated: (workspaceId: string, type: WorkspaceCreationType) => void;
}) {
  const [step, setStep] = useState<CreateStep>("pick");
  const [error, setError] = useState<string | null>(null);
  const [workspaceName, setWorkspaceName] = useState("");

  // Reset state every time the dialog opens so stale "loading" step doesn't persist.
  useEffect(() => {
    if (open) {
      setStep("pick");
      setError(null);
      setWorkspaceName("");
    }
  }, [open]);

  const handleClose = () => {
    setStep("pick");
    setError(null);
    setWorkspaceName("");
    onClose();
  };

  const handleDemo = async () => {
    setStep("loading");
    setError(null);
    try {
      const result = await OnboardingService.setupDemo();
      onCreated(result.workspace_id, "demo");
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to set up demo workspace");
      setStep("pick");
    }
  };

  const handleNewCreate = async () => {
    setStep("loading");
    setError(null);
    try {
      const result = await OnboardingService.setupNew(workspaceName.trim() || undefined);
      onCreated(result.workspace_id, "new");
    } catch (err) {
      setError(extractErrorMessage(err) ?? "Failed to create workspace");
      setStep("new");
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && handleClose()}>
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='font-semibold text-base'>
            {step === "github"
              ? "Import from GitHub"
              : step === "new"
                ? "Create blank workspace"
                : "New workspace"}
          </DialogTitle>
        </DialogHeader>

        {step === "loading" && (
          <div className='flex flex-col items-center gap-3 py-10'>
            <Loader2 className='h-5 w-5 animate-spin text-primary' />
            <p className='text-muted-foreground text-sm'>Setting up workspace…</p>
          </div>
        )}

        {step === "pick" && (
          <div className='flex flex-col gap-5'>
            <div className='flex flex-col gap-2'>
              <OptionCard
                label='01'
                icon={<GithubIcon className='h-3.5 w-3.5' />}
                title='Import from GitHub'
                description='Clone an existing repository and start working immediately.'
                badge='Recommended'
                onClick={() => setStep("github")}
              />
              <OptionCard
                label='02'
                icon={<BookOpen className='h-3.5 w-3.5' />}
                title='Demo Workspace'
                description='Explore Oxy with pre-built sample data and example queries.'
                onClick={handleDemo}
              />
              <OptionCard
                label='03'
                icon={<Plus className='h-3.5 w-3.5' />}
                title='Blank Workspace'
                description='Start from scratch with an empty workspace.'
                onClick={() => {
                  setWorkspaceName("");
                  setError(null);
                  setStep("new");
                }}
              />
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}
          </div>
        )}

        {step === "new" && (
          <div className='flex flex-col gap-4'>
            <div className='space-y-1.5'>
              <Label htmlFor='new-ws-name'>Workspace name</Label>
              <Input
                id='new-ws-name'
                placeholder='my-workspace'
                value={workspaceName}
                onChange={(e) => setWorkspaceName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleNewCreate();
                }}
                autoFocus
              />
              <p className='text-muted-foreground text-xs'>Leave blank for a default name.</p>
            </div>

            {error && (
              <p className='rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-center text-destructive text-sm'>
                {error}
              </p>
            )}

            <div className='flex gap-2'>
              <Button
                variant='outline'
                size='sm'
                className='flex-1 text-xs'
                onClick={() => {
                  setError(null);
                  setStep("pick");
                }}
              >
                Back
              </Button>
              <Button size='sm' className='flex-1 text-xs' onClick={handleNewCreate}>
                Create workspace
              </Button>
            </div>
          </div>
        )}

        {step === "github" && (
          <GitHubOnboardingStep
            onBack={() => setStep("pick")}
            onDone={(workspaceId) => onCreated(workspaceId, "github")}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function OptionCard({
  label,
  icon,
  title,
  description,
  badge,
  onClick
}: {
  label: string;
  icon: React.ReactNode;
  title: string;
  description: string;
  badge?: string;
  onClick: () => void;
}) {
  return (
    <button
      type='button'
      onClick={onClick}
      className='group flex items-center gap-4 rounded-lg border border-border bg-transparent px-4 py-3.5 text-left transition-all hover:border-primary/40 hover:bg-primary/[0.03]'
    >
      <span className='shrink-0 font-mono text-[11px] text-muted-foreground/50 tabular-nums'>
        {label}
      </span>
      <div className='flex min-w-0 flex-1 flex-col gap-0.5'>
        <div className='flex items-center gap-2'>
          <span className='font-medium text-foreground text-sm transition-colors group-hover:text-primary'>
            {title}
          </span>
          {badge && (
            <span className='rounded-full bg-primary/10 px-2 py-0.5 font-medium text-[10px] text-primary'>
              {badge}
            </span>
          )}
        </div>
        <span className='text-muted-foreground text-xs leading-relaxed'>{description}</span>
      </div>
      <span className='shrink-0 text-muted-foreground/30 transition-colors group-hover:text-primary/60'>
        {icon}
      </span>
    </button>
  );
}

// ─── Workspace card ─────────────────────────────────────────────────────────────

function formatRelativeDate(dateStr: string | null): string | null {
  if (!dateStr) return null;
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return `${diffDays} days ago`;
  return date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function WorkspaceCard({
  workspace,
  index,
  onSwitch,
  onDelete,
  isSwitching,
  isDeleting
}: {
  workspace: WorkspaceSummary;
  index: number;
  onSwitch: () => void;
  onDelete: () => void;
  isSwitching: boolean;
  isDeleting: boolean;
}) {
  const createdAt = formatRelativeDate(workspace.created_at);
  const isCloning = workspace.is_cloning;
  const cloneError = workspace.clone_error;
  const isErrored = !!cloneError;
  const isDisabled = isCloning || isErrored;

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
    renameWorkspace(
      { id: workspace.id, name },
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
              : workspace.active
                ? "border-primary/30 bg-primary/[0.02] shadow-sm hover:shadow-sm"
                : "border-border bg-card hover:border-border/60 hover:shadow-sm"
        }`}
      >
        {/* Active accent line */}
        {workspace.active && !isDisabled && (
          <div className='absolute top-0 right-0 left-0 h-[2px] bg-primary' />
        )}

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
            disabled={isSwitching || isDisabled}
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
                    : workspace.active
                      ? "text-primary"
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
                {!isDisabled && workspace.active && !isSwitching && (
                  <span className='rounded-full bg-primary/10 px-2.5 py-1 font-medium text-[11px] text-primary'>
                    Active
                  </span>
                )}
                {!isDisabled && isSwitching && (
                  <Loader2 className='h-3.5 w-3.5 animate-spin text-muted-foreground' />
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
                Repository is being cloned — activate once complete.
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

// ─── Page ─────────────────────────────────────────────────────────────────────

export default function WorkspacesPage() {
  const navigate = useNavigate();
  const { data: workspaces = [], isPending, isError, refetch } = useAllWorkspaces();
  const { mutate: deleteWorkspace, isPending: isDeleting } = useDeleteWorkspace();
  const {
    mutate: activateWorkspace,
    variables: activatingId,
    isPending: isActivating
  } = useActivateWorkspace();
  const [createOpen, setCreateOpen] = useState(false);
  const [reminderWorkspaceId, setReminderWorkspaceId] = useState<string | null>(null);
  const [reminderType, setReminderType] = useState<WorkspaceCreationType | null>(null);

  const handleSwitch = (workspace: WorkspaceSummary) => {
    if (workspace.active) {
      navigate(ROUTES.ROOT);
    } else {
      activateWorkspace(workspace.id, { onSuccess: () => navigate(ROUTES.ROOT) });
    }
  };

  const handleDelete = (workspace: WorkspaceSummary) => {
    deleteWorkspace(
      { id: workspace.id, deleteFiles: true },
      {
        onSuccess: async () => {
          const updated = await refetch();
          if ((updated.data?.length ?? 0) === 0) {
            navigate(ROUTES.SETUP);
          }
        }
      }
    );
  };

  const handleCreated = (workspaceId: string, type: WorkspaceCreationType) => {
    setCreateOpen(false);
    refetch();
    // GitHub workspaces are still cloning — don't activate yet, let the user do it manually.
    if (type !== "github") {
      activateWorkspace(workspaceId);
    }
    setReminderWorkspaceId(workspaceId);
    setReminderType(type);
  };

  if (isPending) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
      </div>
    );
  }

  if (isError) {
    return (
      <div className='flex h-full w-full items-center justify-center'>
        <p className='text-destructive text-sm'>Failed to load workspaces.</p>
      </div>
    );
  }

  return (
    <div className='mx-auto w-full max-w-4xl px-6 py-12'>
      {/* Page header */}
      <div className='mb-8 flex items-end justify-between'>
        <h1 className='font-semibold text-2xl tracking-tight'>Workspaces</h1>
        <Button onClick={() => setCreateOpen(true)} size='sm' className='h-8 gap-1.5 px-3 text-xs'>
          <Plus className='h-3.5 w-3.5' />
          New workspace
        </Button>
      </div>

      {/* Workspace grid */}
      <ul className='grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3'>
        {workspaces.map((workspace, index) => (
          <WorkspaceCard
            key={workspace.id}
            workspace={workspace}
            index={index}
            onSwitch={() => handleSwitch(workspace)}
            onDelete={() => handleDelete(workspace)}
            isSwitching={isActivating && activatingId === workspace.id}
            isDeleting={isDeleting}
          />
        ))}
        <li
          className='fade-in slide-in-from-bottom-2 h-full animate-in fill-mode-both duration-300'
          style={{ animationDelay: `${workspaces.length * 60}ms` }}
        >
          <button
            type='button'
            onClick={() => setCreateOpen(true)}
            className='group flex h-full min-h-[130px] w-full flex-col items-center justify-center gap-2.5 rounded-xl border border-border/50 border-dashed bg-card transition-all hover:border-primary/40 hover:bg-primary/[0.02]'
          >
            <div className='flex h-8 w-8 items-center justify-center rounded-full border border-border transition-all group-hover:border-primary/40 group-hover:bg-primary/10'>
              <Plus className='h-4 w-4 text-muted-foreground/60 transition-colors group-hover:text-primary' />
            </div>
            <span className='font-medium text-muted-foreground/60 text-sm transition-colors group-hover:text-primary'>
              New workspace
            </span>
          </button>
        </li>
      </ul>

      <CreateWorkspaceDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        onCreated={handleCreated}
      />

      <WorkspaceReminderDialog
        open={reminderWorkspaceId !== null}
        workspaceId={reminderWorkspaceId}
        workspaceType={reminderType}
        onClose={() => {
          const id = reminderWorkspaceId;
          const type = reminderType;
          setReminderWorkspaceId(null);
          setReminderType(null);
          // GitHub workspaces are still cloning — stay on the list page.
          // For demo/new workspaces the workspace is immediately usable, navigate to the IDE.
          if (type !== "github" && id) {
            navigate(ROUTES.WORKSPACE(id).IDE.ROOT);
          }
        }}
      />
    </div>
  );
}
