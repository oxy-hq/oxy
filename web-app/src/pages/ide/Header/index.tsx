import {
  ChevronDown,
  Download,
  GitBranch,
  GitMerge,
  GitPullRequest,
  History,
  Home,
  RefreshCw,
  RotateCcw,
  Upload
} from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/shadcn/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/shadcn/popover";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { useAuth } from "@/contexts/AuthContext";
import useDiffSummary from "@/hooks/api/files/useDiffSummary";
import { useForcePush, usePushChanges } from "@/hooks/api/projects/useProjects";
import useRevisionInfo from "@/hooks/api/projects/useRevisionInfo";
import { useGithubSettings } from "@/hooks/api/useGithubSettings";
import ROUTES from "@/libs/utils/routes";
import { type CommitEntry, ProjectService } from "@/services/api";
import useCurrentProject from "@/stores/useCurrentProject";
import useIdeBranch from "@/stores/useIdeBranch";
import { BranchInfo } from "./BranchInfo";
import { BranchQuickSwitcher } from "./BranchQuickSwitcher";
import { BranchSettings } from "./BranchSettings";
import { PullDialog } from "./BranchSettings/BranchInfo/Actions/PullDialog";
import { ChangesPanel } from "./ChangesPanel";

const GithubIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox='0 0 24 24' fill='currentColor' aria-hidden='true'>
    <path d='M12 2C6.477 2 2 6.477 2 12c0 4.418 2.865 8.166 6.839 9.489.5.092.682-.217.682-.482 0-.237-.009-.868-.013-1.703-2.782.603-3.369-1.342-3.369-1.342-.454-1.154-1.11-1.462-1.11-1.462-.908-.62.069-.608.069-.608 1.003.07 1.532 1.031 1.532 1.031.891 1.528 2.341 1.087 2.91.831.091-.645.349-1.087.635-1.337-2.22-.252-4.555-1.11-4.555-4.943 0-1.091.39-1.984 1.03-2.682-.103-.253-.447-1.27.097-2.646 0 0 .84-.269 2.75 1.025A9.578 9.578 0 0112 6.836a9.59 9.59 0 012.504.337c1.909-1.294 2.748-1.025 2.748-1.025.546 1.376.202 2.394.1 2.646.64.699 1.026 1.591 1.026 2.682 0 3.841-2.337 4.687-4.565 4.935.359.309.678.919.678 1.852 0 1.337-.012 2.414-.012 2.742 0 .267.18.578.688.48C19.138 20.163 22 16.418 22 12c0-5.523-4.477-10-10-10z' />
  </svg>
);

export const OPEN_BRANCH_SETTINGS = "ide:open-branch-settings";

export const Header = () => {
  const { authConfig } = useAuth();
  const { project } = useCurrentProject();
  const { setOpen } = useSidebar();
  const navigate = useNavigate();
  const [isBranchSettingOpen, setIsBranchSettingOpen] = useState(false);
  const [isBranchPickerOpen, setIsBranchPickerOpen] = useState(false);
  const [pullDialogOpen, setPullDialogOpen] = useState(false);
  const [changesPanelOpen, setChangesPanelOpen] = useState(false);
  const [resetPopoverOpen, setResetPopoverOpen] = useState(false);
  const [recentCommits, setRecentCommits] = useState<CommitEntry[]>([]);
  const [commitsLoading, setCommitsLoading] = useState(false);
  const [resettingHash, setResettingHash] = useState<string | null>(null);

  // Derive the current IDE branch without throwing (useCurrentProjectBranch throws if no project).
  const { getCurrentBranch } = useIdeBranch();
  const activeBranch = project?.active_branch?.name ?? "";
  const ideBranch = project ? (getCurrentBranch(project.id) ?? activeBranch) : activeBranch;
  const defaultBranch = authConfig.default_branch ?? "main";
  const isOnMain = ideBranch === defaultBranch;

  const isLocalOnly = !!authConfig.local_git && !authConfig.cloud;
  const pushLabel = isLocalOnly ? (authConfig.git_remote ? "Commit & Push" : "Commit") : "Push";

  const {
    data: diffSummary,
    refetch: refetchDiff,
    isFetching: isDiffFetching
  } = useDiffSummary(!isOnMain && !!project?.id);
  const { data: githubSettings } = useGithubSettings(authConfig.cloud, false, false);
  const {
    data: revisionInfo,
    refetch: refetchRevision,
    isFetching: isRevisionFetching
  } = useRevisionInfo(authConfig.local_git && !!project?.id);
  const hasLocalChanges = !isOnMain && (diffSummary?.length ?? 0) > 0;

  const isBehind = revisionInfo?.sync_status === "behind";
  const isAhead = revisionInfo?.sync_status === "ahead";
  const isConflict = revisionInfo?.sync_status === "conflict";
  const hasRemote = authConfig.cloud || !!authConfig.git_remote;
  const showPull = hasRemote && isBehind && !isConflict;
  const isFetching = isDiffFetching || isRevisionFetching;

  const handleFetch = () => {
    refetchDiff();
    refetchRevision();
  };

  const pushMutation = usePushChanges();
  const forcePushMutation = useForcePush();
  const handlePush = async (commitMessage: string) => {
    if (!project?.id || !ideBranch) return;
    try {
      const result = await pushMutation.mutateAsync({
        projectId: project.id,
        branchName: ideBranch,
        commitMessage
      });
      if (result.success) {
        toast.success(result.message || "Changes pushed");
        await Promise.all([refetchDiff(), refetchRevision()]);
      } else {
        toast.error(result.message || "Push failed");
      }
    } catch {
      toast.error("Push failed");
    }
  };

  // Build GitHub URLs from either the local remote URL or cloud repository name
  const { githubRepoUrl, githubPrUrl } = (() => {
    const buildBase = (): string | null => {
      const remoteUrl = revisionInfo?.remote_url;
      if (remoteUrl) {
        const match = remoteUrl.match(/github\.com[/:]([^/]+\/[^/.]+?)(?:\.git)?$/);
        if (match) return `https://github.com/${match[1]}`;
      }
      if (githubSettings?.repository_name) {
        return `https://github.com/${githubSettings.repository_name}`;
      }
      return null;
    };

    const base = buildBase();
    if (!base) return { githubRepoUrl: null, githubPrUrl: null };
    return {
      githubRepoUrl: `${base}/tree/${ideBranch}`,
      githubPrUrl: isOnMain ? null : `${base}/compare/${defaultBranch}...${ideBranch}?expand=1`
    };
  })();

  // Show "Open PR" only when not on main, has a GitHub URL, and all commits are pushed
  const showOpenPr = !isOnMain && !!githubPrUrl && !hasLocalChanges && !isConflict && !isAhead;

  const handleOpenResetPopover = async (open: boolean) => {
    setResetPopoverOpen(open);
    if (open && project?.id && ideBranch) {
      setCommitsLoading(true);
      try {
        const result = await ProjectService.getRecentCommits(project.id, ideBranch);
        setRecentCommits(result.commits);
      } catch {
        setRecentCommits([]);
      } finally {
        setCommitsLoading(false);
      }
    }
  };

  const handleResetToCommit = async (hash: string) => {
    if (!project?.id || !ideBranch) return;
    setResettingHash(hash);
    try {
      const result = await ProjectService.resetToCommit(project.id, ideBranch, hash);
      if (result.success) {
        toast.success(`Restored to ${hash.substring(0, 7)}`);
        setResetPopoverOpen(false);
        setChangesPanelOpen(false);
        await Promise.all([refetchDiff(), refetchRevision()]);
      } else {
        toast.error(result.message || "Restore failed");
      }
    } catch {
      toast.error("Restore failed");
    } finally {
      setResettingHash(null);
    }
  };

  const handleForcePush = async () => {
    if (!project?.id || !ideBranch) return;
    try {
      const result = await forcePushMutation.mutateAsync({
        projectId: project.id,
        branchName: ideBranch
      });
      if (result.success) {
        toast.success("Force pushed successfully");
        refetchRevision();
      } else {
        toast.error(result.message || "Force push failed");
      }
    } catch {
      toast.error("Force push failed");
    }
  };

  const handleAbortConflict = async () => {
    if (!project?.id || !ideBranch) return;
    try {
      const result = await ProjectService.abortRebase(project.id, ideBranch);
      if (result.success) {
        toast.success("Rebase aborted — branch restored to previous state");
        setChangesPanelOpen(false);
        refetchRevision();
        refetchDiff();
      } else {
        toast.error(result.message || "Failed to abort");
      }
    } catch {
      toast.error("Failed to abort");
    }
  };

  const handleContinueRebase = async () => {
    if (!project?.id || !ideBranch) return;
    try {
      const result = await ProjectService.continueRebase(project.id, ideBranch);
      if (result.success) {
        toast.success("Conflicts resolved — rebase complete");
        setChangesPanelOpen(false);
        refetchRevision();
        refetchDiff();
      } else {
        toast.error(result.message || "Failed to continue rebase");
      }
    } catch {
      toast.error("Failed to continue rebase");
    }
  };

  useEffect(() => {
    const handler = () => setIsBranchSettingOpen(true);
    window.addEventListener(OPEN_BRANCH_SETTINGS, handler);
    return () => window.removeEventListener(OPEN_BRANCH_SETTINGS, handler);
  }, []);

  const renderContent = () => {
    // Pure local mode: no git — just show a label
    if (!authConfig.local_git && !authConfig.cloud) {
      return <div className='text-muted-foreground text-sm'>Local mode</div>;
    }

    const showBranchInfo = authConfig.local_git || !!project?.project_repo_id;

    // Cloud mode with no repo linked — open the connect-repository dialog directly.
    if (!showBranchInfo) {
      return (
        <button
          type='button'
          onClick={() => setIsBranchSettingOpen(true)}
          className='flex h-7 items-center gap-1.5 rounded border border-border/50 bg-transparent px-2.5 text-sm transition-colors hover:border-border hover:bg-accent/40'
        >
          <GithubIcon className='h-3.5 w-3.5 text-muted-foreground' />
          <span className='text-muted-foreground text-sm'>Connect repository</span>
          <ChevronDown className='ml-0.5 h-3 w-3 flex-shrink-0 text-muted-foreground/60' />
        </button>
      );
    }

    const pill = (
      <button
        type='button'
        className='flex h-7 max-w-36 items-center gap-1.5 overflow-hidden rounded border border-border/50 bg-transparent px-2 text-sm transition-colors hover:border-border hover:bg-accent/40'
      >
        <span className='min-w-0 flex-1'>
          <BranchInfo />
        </span>
        <ChevronDown className='h-3 w-3 flex-shrink-0 text-muted-foreground/60' />
      </button>
    );

    // Determine primary CTA state (priority order)
    // "commit" = has uncommitted changes → open ChangesPanel to review + commit
    // "push"   = committed but not yet pushed → push directly without panel
    type CtaState = "conflict" | "commit" | "push" | "pull" | "pr" | "fetch" | "none";
    const ctaState: CtaState = isConflict
      ? "conflict"
      : !isOnMain && hasLocalChanges
        ? "commit"
        : !isOnMain && isAhead
          ? "push"
          : showPull
            ? "pull"
            : showOpenPr
              ? "pr"
              : !isOnMain && hasRemote
                ? "fetch"
                : "none";

    const showSplit =
      hasRemote && (ctaState === "commit" || ctaState === "push" || ctaState === "pr") && !isOnMain;

    const splitTriggerClass =
      "border-l border-white/20 bg-gradient-to-b from-[#3550FF] to-[#2A40CC] text-white hover:from-[#5D73FF] hover:to-[#3550FF]";

    return (
      <div className='flex items-center gap-1.5'>
        {/* ── GitHub link ─────────────────────────────────────────── */}
        {githubRepoUrl && (
          <a
            href={githubRepoUrl}
            target='_blank'
            rel='noopener noreferrer'
            title={githubSettings?.repository_name ?? "Open on GitHub"}
            className='flex h-7 w-7 items-center justify-center rounded text-muted-foreground/60 transition-colors hover:bg-accent/40 hover:text-foreground'
          >
            <GithubIcon className='h-3.5 w-3.5' />
          </a>
        )}

        {/* ── Branch pill ─────────────────────────────────────────── */}
        <BranchQuickSwitcher
          trigger={pill}
          open={isBranchPickerOpen}
          onOpenChange={setIsBranchPickerOpen}
        />

        {authConfig.local_git && (
          <Popover open={resetPopoverOpen} onOpenChange={handleOpenResetPopover}>
            <PopoverTrigger asChild>
              <button
                type='button'
                title='View commit history'
                className='flex h-7 items-center gap-1 rounded border border-border/50 px-2 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
              >
                <History className='h-3 w-3' />
                History
              </button>
            </PopoverTrigger>
            <PopoverContent className='w-80 p-0' align='end' sideOffset={6}>
              <div className='border-b px-3 py-2'>
                <p className='font-medium text-sm'>Recent commits</p>
                <p className='text-[11px] text-muted-foreground'>
                  Select a commit to restore to it. A new commit will be created with those file
                  contents.
                </p>
              </div>
              <div className='max-h-72 overflow-y-auto'>
                {commitsLoading ? (
                  <div className='flex items-center justify-center py-6 text-muted-foreground text-xs'>
                    Loading…
                  </div>
                ) : recentCommits.length === 0 ? (
                  <div className='flex items-center justify-center py-6 text-muted-foreground text-xs'>
                    No commits found
                  </div>
                ) : (
                  recentCommits.map((c) => (
                    <div
                      key={c.hash}
                      className='group flex items-start gap-2 border-b px-3 py-2 last:border-0 hover:bg-accent/40'
                    >
                      <div className='min-w-0 flex-1'>
                        <p className='truncate text-xs'>{c.message}</p>
                        <p className='font-mono text-[10px] text-muted-foreground'>
                          {c.short_hash} · {c.author} · {c.date}
                        </p>
                      </div>
                      <button
                        type='button'
                        onClick={() => handleResetToCommit(c.hash)}
                        disabled={!!resettingHash}
                        title={`Restore to ${c.short_hash}`}
                        className='mt-0.5 hidden shrink-0 items-center gap-1 rounded bg-primary px-1.5 py-0.5 text-[10px] text-white transition-colors hover:bg-primary/80 disabled:opacity-50 group-hover:flex'
                      >
                        <RotateCcw className='h-2.5 w-2.5' />
                        {resettingHash === c.hash ? "…" : "Restore"}
                      </button>
                    </div>
                  ))
                )}
              </div>
            </PopoverContent>
          </Popover>
        )}

        <div className='mx-0.5 h-4 w-px bg-border/50' />

        {/* ── On main: new branch shortcut ────────────────────────── */}
        {isOnMain && (
          <button
            type='button'
            onClick={() => setIsBranchPickerOpen(true)}
            title='Create a new branch'
            className='flex h-7 items-center gap-1 rounded border border-primary/30 bg-primary/8 px-2 text-primary text-xs transition-colors hover:border-primary/50 hover:bg-primary/15'
          >
            <GitBranch className='h-3 w-3' />
            New branch
          </button>
        )}

        {/* ── Primary CTA ─────────────────────────────────────────── */}
        {ctaState !== "none" && !isOnMain && (
          <div className='flex h-7 items-stretch'>
            {ctaState === "conflict" && (
              <button
                type='button'
                onClick={() => setChangesPanelOpen(true)}
                className='flex items-center gap-1 rounded border border-amber-500/30 bg-amber-500/10 px-2.5 text-amber-400 text-xs transition-colors hover:border-amber-500/50 hover:bg-amber-500/20'
              >
                <GitMerge className='h-3 w-3' />
                Conflict
              </button>
            )}

            {ctaState === "commit" && (
              <button
                type='button'
                onClick={() => setChangesPanelOpen(true)}
                disabled={pushMutation.isPending}
                data-testid='ide-commit-push-button'
                className={`flex items-center gap-1 bg-gradient-to-b from-[#3550FF] to-[#2A40CC] px-2.5 font-medium text-white text-xs shadow-[#0B1033]/40 shadow-sm transition-all hover:from-[#5D73FF] hover:to-[#3550FF] disabled:opacity-50 ${showSplit ? "rounded-l" : "rounded"}`}
              >
                <Upload className='h-3 w-3' />
                {pushLabel}
              </button>
            )}

            {ctaState === "push" && (
              <button
                type='button'
                onClick={() => handlePush("")}
                disabled={pushMutation.isPending}
                data-testid='ide-push-button'
                className={`flex items-center gap-1 bg-gradient-to-b from-[#3550FF] to-[#2A40CC] px-2.5 font-medium text-white text-xs shadow-[#0B1033]/40 shadow-sm transition-all hover:from-[#5D73FF] hover:to-[#3550FF] disabled:opacity-50 ${showSplit ? "rounded-l" : "rounded"}`}
              >
                <Upload className='h-3 w-3' />
                {pushMutation.isPending ? "Pushing…" : "Push"}
              </button>
            )}

            {ctaState === "pull" && (
              <>
                <button
                  type='button'
                  onClick={() => setPullDialogOpen(true)}
                  data-testid='ide-pull-button'
                  className='flex items-center gap-1 rounded-l border border-border/50 px-2.5 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
                >
                  <Download className='h-3 w-3' />
                  Pull
                </button>
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <button
                      type='button'
                      aria-label='More actions'
                      className='flex w-5 items-center justify-center rounded-r border border-border/50 border-l-0 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground'
                    >
                      <ChevronDown className='h-2.5 w-2.5' />
                    </button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align='end' className='w-auto min-w-0'>
                    <DropdownMenuItem
                      onClick={handleForcePush}
                      disabled={forcePushMutation.isPending}
                      className='gap-1.5 px-2 py-1 text-destructive text-xs focus:text-destructive'
                    >
                      <Upload className='h-3 w-3' />
                      {forcePushMutation.isPending ? "Pushing…" : "Force Push"}
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </>
            )}

            {ctaState === "pr" && (
              <a
                href={githubPrUrl ?? ""}
                target='_blank'
                rel='noopener noreferrer'
                title='Open pull request on GitHub'
                className={`flex items-center gap-1 bg-gradient-to-b from-[#3550FF] to-[#2A40CC] px-2.5 font-medium text-white text-xs shadow-[#0B1033]/40 shadow-sm transition-all hover:from-[#5D73FF] hover:to-[#3550FF] ${showSplit ? "rounded-l" : "rounded"}`}
              >
                <GitPullRequest className='h-3 w-3' />
                Open PR
              </a>
            )}

            {ctaState === "fetch" && (
              <button
                type='button'
                onClick={handleFetch}
                disabled={isFetching}
                title='Fetch from origin'
                data-testid='ide-git-refresh-button'
                className='flex h-7 items-center gap-1 rounded border border-border/50 px-2.5 text-muted-foreground text-xs transition-colors hover:border-border hover:bg-accent/40 hover:text-foreground disabled:opacity-40'
              >
                <RefreshCw className={`h-3 w-3 ${isFetching ? "animate-spin" : ""}`} />
                Fetch
              </button>
            )}

            {/* Split ▼ — Fetch as secondary action */}
            {showSplit && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button
                    type='button'
                    aria-label='More actions'
                    className={`flex w-5 items-center justify-center rounded-r text-xs transition-all ${splitTriggerClass}`}
                  >
                    <ChevronDown className='h-2.5 w-2.5' />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align='end' className='w-auto min-w-0'>
                  <DropdownMenuItem
                    onClick={handleFetch}
                    disabled={isFetching}
                    className='gap-1.5 px-2 py-1 text-xs'
                  >
                    <RefreshCw className={`h-3 w-3 ${isFetching ? "animate-spin" : ""}`} />
                    {isFetching ? "Fetching…" : "Fetch"}
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            )}
          </div>
        )}
      </div>
    );
  };

  const homeRoute = project?.id ? ROUTES.PROJECT(project.id).HOME : ROUTES.ROOT;

  const handleHomeClick = () => {
    setOpen(true);
    navigate(homeRoute);
  };

  return (
    <Card className='flex gap-2 rounded-none border-b bg-sidebar-background p-1 shadow-none'>
      <Button
        variant='ghost'
        size='sm'
        onClick={handleHomeClick}
        tooltip={{ content: "Back to Home", side: "right" }}
        className='h-8 w-8'
      >
        <Home className='h-4 w-4' />
      </Button>
      <div className='flex flex-1 items-center justify-end'>{renderContent()}</div>

      <BranchSettings isOpen={isBranchSettingOpen} onClose={() => setIsBranchSettingOpen(false)} />
      <PullDialog open={pullDialogOpen} onOpenChange={setPullDialogOpen} />
      <ChangesPanel
        open={changesPanelOpen}
        onOpenChange={setChangesPanelOpen}
        diffSummary={diffSummary ?? []}
        isPushing={pushMutation.isPending}
        pushLabel={pushLabel}
        onPush={handlePush}
        isConflict={isConflict}
        onAbortConflict={handleAbortConflict}
        onContinueRebase={handleContinueRebase}
        onConflictResolved={handleFetch}
      />
    </Card>
  );
};

export default Header;
