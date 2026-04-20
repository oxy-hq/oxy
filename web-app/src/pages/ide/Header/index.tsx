import { Home } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { Card } from "@/components/ui/shadcn/card";
import useSidebar from "@/components/ui/shadcn/sidebar-context";
import { FEATURES } from "@/libs/features";
import ROUTES from "@/libs/utils/routes";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import useIdeBranch from "@/stores/useIdeBranch";
import useSelectedRepo from "@/stores/useSelectedRepo";
import { ChangesPanel } from "./ChangesPanel";
import { GitActions } from "./components/GitActions";
import { GithubIcon } from "./components/GithubIcon";
import { IDEProjectSwitcher } from "./components/IDEProjectSwitcher";
import { LinkedRepoActions } from "./components/LinkedRepoActions";
import { RepoSwitcher } from "./components/RepoSwitcher";
import { useGithubUrls } from "./hooks/useGithubUrls";
import { useGitMutations } from "./hooks/useGitMutations";
import { PullDialog } from "./PullDialog";

export const OPEN_BRANCH_SETTINGS = "ide:open-branch-settings";

export const Header = () => {
  const { workspace: project } = useCurrentWorkspace();
  const { selectedRepo } = useSelectedRepo();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const isLinkedRepo = selectedRepo !== "primary";
  const { setOpen } = useSidebar();
  const navigate = useNavigate();
  const [pullDialogOpen, setPullDialogOpen] = useState(false);
  const [changesPanelOpen, setChangesPanelOpen] = useState(false);

  // Resolve current IDE branch without throwing — useCurrentWorkspaceBranch
  // requires a workspace, but Header renders during initial load too.
  const { getCurrentBranch } = useIdeBranch();
  const activeBranch = project?.active_branch?.name ?? "";
  const ideBranch = project ? (getCurrentBranch(project.id) ?? activeBranch) : activeBranch;
  const defaultBranch = project?.default_branch ?? "main";
  const isOnMain = ideBranch === defaultBranch;

  const caps = project?.capabilities;
  const canCommit = !!caps?.can_commit;
  const canPush = !!caps?.can_push;
  const canPull = !!caps?.can_pull;
  const canBrowseHistory = !!caps?.can_browse_history;
  const canDiff = !!caps?.can_diff;
  const pushLabel = canCommit ? (canPush ? "Commit & Push" : "Commit") : "Push";

  const {
    diffSummary,
    revisionInfo,
    isFetching,
    isPushing,
    isForcePushing,
    fetchAll,
    push,
    forcePush,
    abortRebase,
    continueRebase
  } = useGitMutations({
    workspaceId: project?.id,
    branch: ideBranch,
    enableDiff: canDiff || !isOnMain,
    enableRevision: canDiff
  });

  const hasLocalChanges = canCommit && (diffSummary?.length ?? 0) > 0;
  const isConflict = revisionInfo?.sync_status === "conflict";

  const { repoUrl: githubRepoUrl, prUrl: githubPrUrl } = useGithubUrls({
    remoteUrl: revisionInfo?.remote_url,
    branch: ideBranch,
    defaultBranch,
    isOnMain
  });

  const handleHomeClick = () => {
    setOpen(true);
    navigate(project?.id ? ROUTES.ORG(orgSlug).WORKSPACE(project.id).HOME : ROUTES.ROOT);
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
      <IDEProjectSwitcher />
      {githubRepoUrl && !isLinkedRepo && (
        <a
          href={githubRepoUrl}
          target='_blank'
          rel='noopener noreferrer'
          title='Open on GitHub'
          className='flex h-7 w-7 items-center justify-center rounded text-muted-foreground/60 transition-colors hover:bg-accent/40 hover:text-foreground'
        >
          <GithubIcon className='h-3.5 w-3.5' />
        </a>
      )}
      <div className='flex flex-1 items-center justify-end gap-2'>
        {FEATURES.LINKED_REPOS && <RepoSwitcher isReadOnly={!canCommit && !project?.id} />}
        {FEATURES.LINKED_REPOS && isLinkedRepo ? (
          <LinkedRepoActions repoName={selectedRepo} />
        ) : (
          <GitActions
            workspaceId={project?.id}
            branch={ideBranch}
            isOnMain={isOnMain}
            canCommit={canCommit}
            canBrowseHistory={canBrowseHistory}
            canPush={canPush}
            canPull={canPull}
            hasLocalChanges={hasLocalChanges}
            revisionInfo={revisionInfo}
            prUrl={githubPrUrl}
            pushLabel={pushLabel}
            isPushing={isPushing}
            isForcePushing={isForcePushing}
            isFetching={isFetching}
            onOpenChanges={() => setChangesPanelOpen(true)}
            onOpenPullDialog={() => setPullDialogOpen(true)}
            onPushDirect={() => push("")}
            onForcePush={forcePush}
            onFetch={fetchAll}
            onResetSuccess={async () => {
              setChangesPanelOpen(false);
              await fetchAll();
            }}
          />
        )}
      </div>

      <PullDialog open={pullDialogOpen} onOpenChange={setPullDialogOpen} />
      <ChangesPanel
        open={changesPanelOpen}
        onOpenChange={setChangesPanelOpen}
        diffSummary={diffSummary ?? []}
        isPushing={isPushing}
        pushLabel={pushLabel}
        onPush={push}
        isConflict={isConflict}
        onAbortConflict={async () => {
          await abortRebase();
          setChangesPanelOpen(false);
        }}
        onContinueRebase={async () => {
          await continueRebase();
          setChangesPanelOpen(false);
        }}
        onConflictResolved={fetchAll}
      />
    </Card>
  );
};

export default Header;
